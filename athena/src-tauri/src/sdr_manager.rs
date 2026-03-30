//! RTL-SDR via host `rtl_fm`: process management, stderr logging, loopback HTTP PCM forwarder.
//! No demodulation in Rust; no audio playback crates.

use crossbeam_channel::{bounded, Receiver, Sender};
use serde::Serialize;
use std::io::{self, BufRead, Read, Write};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const PCM_QUEUE_CAP: usize = 128;
const READ_BUF: usize = 8192;
const STDERR_TAIL_MAX: usize = 200;

pub static SDR_STATE: Mutex<Option<SdrRuntime>> = Mutex::new(None);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SdrStartResult {
    pub port: u16,
    pub sample_rate_hz: u32,
    pub channels: u8,
    pub stream_url: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SdrDeviceRow {
    pub index: u32,
    pub label: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SdrStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub frequency_hz: Option<u64>,
    pub mode: Option<String>,
    pub last_error: Option<String>,
}

struct SdrCtl {
    stop: AtomicBool,
    child: Mutex<Option<Child>>,
}

impl SdrCtl {
    fn new(child: Child) -> Self {
        Self {
            stop: AtomicBool::new(false),
            child: Mutex::new(Some(child)),
        }
    }

    fn kill_child(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(mut g) = self.child.lock() {
            if let Some(mut c) = g.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
        }
    }
}

pub struct SdrRuntime {
    ctl: Arc<SdrCtl>,
    _handles: Vec<JoinHandle<()>>,
}

impl Drop for SdrRuntime {
    fn drop(&mut self) {
        self.ctl.kill_child();
    }
}

fn emit_log(app: &AppHandle, line: String) {
    let _ = app.emit("athena:sdr-log", line);
}

fn push_stderr_tail(tail: &Mutex<Vec<String>>, line: String) {
    if let Ok(mut v) = tail.lock() {
        v.push(line);
        while v.len() > STDERR_TAIL_MAX {
            v.remove(0);
        }
    }
}

fn spawn_stderr_reader(
    app: AppHandle,
    ctl: Arc<SdrCtl>,
    stderr: std::process::ChildStderr,
    tail: Arc<Mutex<Vec<String>>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = std::io::BufReader::new(stderr);
        let mut buf = Vec::new();
        loop {
            if ctl.stop.load(Ordering::SeqCst) {
                break;
            }
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    let s = String::from_utf8_lossy(&buf).trim().to_string();
                    if !s.is_empty() {
                        push_stderr_tail(&tail, s.clone());
                        emit_log(&app, s);
                    }
                }
                Err(_) => break,
            }
        }
    })
}

fn spawn_stdout_reader(
    ctl: Arc<SdrCtl>,
    forwarding: Arc<AtomicBool>,
    mut stdout: std::process::ChildStdout,
    tx: Sender<Vec<u8>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut buf = vec![0u8; READ_BUF];
        loop {
            if ctl.stop.load(Ordering::SeqCst) {
                break;
            }
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if forwarding.load(Ordering::SeqCst) && tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        drop(tx);
    })
}

fn spawn_http_forwarder(
    app: AppHandle,
    ctl: Arc<SdrCtl>,
    forwarding: Arc<AtomicBool>,
    listener: TcpListener,
    sample_rate: u32,
    rx: Receiver<Vec<u8>>,
    tail: Arc<Mutex<Vec<String>>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let _ = listener.set_nonblocking(true);
        let mut stream = loop {
            if ctl.stop.load(Ordering::SeqCst) {
                return;
            }
            match listener.accept() {
                Ok((s, _)) => break s,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(40));
                }
                Err(e) => {
                    push_stderr_tail(&tail, format!("[sdr] HTTP accept failed: {e}"));
                    finish_sdr_session(&app, "accept_failed");
                    return;
                }
            }
        };

        let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));

        // Drain HTTP request headers from the browser fetch() client.
        {
            let mut tmp = [0u8; 512];
            let mut seen = Vec::new();
            for _ in 0..32 {
                match stream.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => {
                        seen.extend_from_slice(&tmp[..n]);
                        if seen.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                        if seen.len() > 8192 {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }

        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: audio/L16; rate={}; channels=1\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
            sample_rate
        );
        if stream.write_all(header.as_bytes()).is_err() {
            finish_sdr_session(&app, "http_write_headers");
            return;
        }
        let _ = stream.flush();

        forwarding.store(true, Ordering::SeqCst);

        loop {
            if ctl.stop.load(Ordering::SeqCst) {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(chunk) => {
                    if chunk.is_empty() {
                        continue;
                    }
                    if stream.write_all(&chunk).is_err() {
                        push_stderr_tail(
                            &tail,
                            "[sdr] HTTP client disconnected or write failed; stopping rtl_fm"
                                .to_string(),
                        );
                        finish_sdr_session(&app, "client_disconnect");
                        break;
                    }
                    let _ = stream.flush();
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    finish_sdr_session(&app, "pcm_stopped");
                    break;
                }
            }
        }
    })
}

pub fn list_sdr_devices() -> Vec<SdrDeviceRow> {
    let mut out = Vec::new();
    let test_out = Command::new("rtl_test")
        .arg("-t")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    if let Ok(output) = test_out {
        let text = String::from_utf8_lossy(&output.stderr).to_string()
            + &String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let line = line.trim();
            if line.contains("Found") && line.contains("device") {
                continue;
            }
            if line.contains(':') && (line.contains("0x") || line.contains("Serial")) {
                out.push(SdrDeviceRow {
                    index: out.len() as u32,
                    label: line.to_string(),
                });
            }
        }
    }
    if out.is_empty() {
        out.push(SdrDeviceRow {
            index: 0,
            label: "Device 0 (default — use rtl_fm -d if you have multiple dongles)".to_string(),
        });
    }
    out
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdrStartArgs {
    pub frequency_hz: u64,
    pub mode: String,
    pub device_index: Option<u32>,
    pub sample_rate_hz: Option<u32>,
}

#[tauri::command]
pub fn sdr_list_devices() -> Vec<SdrDeviceRow> {
    list_sdr_devices()
}

#[tauri::command]
pub fn sdr_status() -> SdrStatus {
    let guard = SDR_STATE.lock().unwrap();
    if guard.is_none() {
        return SdrStatus {
            running: false,
            port: None,
            frequency_hz: None,
            mode: None,
            last_error: None,
        };
    }
    SdrStatus {
        running: true,
        port: None,
        frequency_hz: None,
        mode: None,
        last_error: None,
    }
}

#[tauri::command]
pub fn sdr_stop(app: AppHandle) {
    stop_sdr_internal(&app);
}

/// Clears global session (drops [`SdrRuntime`] so rtl_fm is killed) and notifies the UI.
pub fn finish_sdr_session(app: &AppHandle, reason: &str) {
    let mut guard = SDR_STATE.lock().unwrap();
    if guard.take().is_some() {
        let _ = app.emit(
            "athena:sdr-stopped",
            serde_json::json!({ "reason": reason }),
        );
    }
}

pub fn stop_sdr_internal(app: &AppHandle) {
    finish_sdr_session(app, "stop_command");
}

#[tauri::command]
pub fn sdr_start(app: AppHandle, args: SdrStartArgs) -> Result<SdrStartResult, String> {
    stop_sdr_internal(&app);

    let sample_rate = args.sample_rate_hz.unwrap_or(48_000).clamp(8_000, 384_000);

    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("bind loopback: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?
        .port();

    let (tx, rx) = bounded::<Vec<u8>>(PCM_QUEUE_CAP);
    let forwarding = Arc::new(AtomicBool::new(false));

    let mode_arg = match args.mode.to_lowercase().as_str() {
        "am" => "am",
        "wbfm" | "widefm" => "wbfm",
        _ => "fm",
    };

    let mut cmd = Command::new("rtl_fm");
    cmd.arg("-f").arg(args.frequency_hz.to_string());
    cmd.arg("-M").arg(mode_arg);
    cmd.arg("-r").arg(sample_rate.to_string());
    cmd.arg("-s").arg("240000");
    if let Some(d) = args.device_index {
        cmd.arg("-d").arg(d.to_string());
    }
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to spawn rtl_fm (is it installed and on PATH?): {e}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "rtl_fm: no stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "rtl_fm: no stderr".to_string())?;

    let ctl = Arc::new(SdrCtl::new(child));
    let tail = Arc::new(Mutex::new(Vec::<String>::new()));

    let h_stderr = spawn_stderr_reader(app.clone(), ctl.clone(), stderr, tail.clone());
    let h_out = spawn_stdout_reader(ctl.clone(), forwarding.clone(), stdout, tx);
    let h_http = spawn_http_forwarder(
        app.clone(),
        ctl.clone(),
        forwarding.clone(),
        listener,
        sample_rate,
        rx,
        tail,
    );

    let runtime = SdrRuntime {
        ctl: ctl.clone(),
        _handles: vec![h_stderr, h_out, h_http],
    };

    *SDR_STATE.lock().unwrap() = Some(runtime);

    Ok(SdrStartResult {
        port,
        sample_rate_hz: sample_rate,
        channels: 1,
        stream_url: format!("http://127.0.0.1:{port}/"),
    })
}
