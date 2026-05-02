//! Discover and validate GGUF models under AthenaData `models/`, and optionally manage a
//! localhost-bound llama.cpp-compatible server subprocess (`ATHENA_LLAMA_SERVER_BIN`).

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const GGUF_MAGIC: &[u8; 4] = b"GGUF";

/// Upper bound for joining the child `wait()` thread during teardown so app exit is not blocked indefinitely.
const MANAGED_LLM_SHUTDOWN_MAX_WAIT: Duration = Duration::from_millis(2500);
const MANAGED_LLM_SHUTDOWN_POLL: Duration = Duration::from_millis(25);

static LOCAL_INFERENCE_CHILD: OnceLock<Mutex<Option<Child>>> = OnceLock::new();
static MANAGED_HTTP_PORT: OnceLock<Arc<Mutex<Option<u16>>>> = OnceLock::new();

fn child_slot() -> &'static Mutex<Option<Child>> {
    LOCAL_INFERENCE_CHILD.get_or_init(|| Mutex::new(None))
}

/// Shared port for the managed llama HTTP server (`http://127.0.0.1:{port}/v1`). Cleared on shutdown.
pub fn managed_llm_port_holder() -> Arc<Mutex<Option<u16>>> {
    MANAGED_HTTP_PORT
        .get_or_init(|| Arc::new(Mutex::new(None)))
        .clone()
}

fn set_managed_port(port: Option<u16>) {
    if let Ok(mut g) = managed_llm_port_holder().lock() {
        *g = port;
    }
}

/// OpenAI-compatible base URL when a managed server is running and settings omit an explicit URL.
pub fn managed_llama_openai_base_url() -> Option<String> {
    let port = managed_llm_port_holder()
        .lock()
        .ok()
        .and_then(|g| *g)?;
    Some(format!("http://127.0.0.1:{port}/v1"))
}

/// Kill the managed local inference child, if any (call from [`RunEvent::Exit`]).
/// Waits only up to [`MANAGED_LLM_SHUTDOWN_MAX_WAIT`] for `wait()`; does not block app exit indefinitely.
pub fn shutdown_managed_local_inference() {
    let child_opt = {
        let Ok(mut slot) = child_slot().lock() else {
            return;
        };
        slot.take()
    };
    set_managed_port(None);
    let Some(mut c) = child_opt else {
        return;
    };
    let _ = c.kill();
    let joiner = thread::spawn(move || {
        let _ = c.wait();
    });
    let start = Instant::now();
    while start.elapsed() < MANAGED_LLM_SHUTDOWN_MAX_WAIT {
        if joiner.is_finished() {
            let _ = joiner.join();
            return;
        }
        thread::sleep(MANAGED_LLM_SHUTDOWN_POLL);
    }
    // Timed out: do not join forever; the helper thread may still reap the child in the background.
    drop(joiner);
}

/// True if the file has a `.gguf` extension and starts with GGUF magic bytes.
pub fn validate_gguf_file(path: &Path) -> Result<(), String> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !name.to_ascii_lowercase().ends_with(".gguf") {
        return Err("Not a .gguf file.".to_string());
    }
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() < 4 {
        return Err("File too small to be GGUF.".to_string());
    }
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hdr = [0u8; 4];
    f.read_exact(&mut hdr).map_err(|e| e.to_string())?;
    if &hdr != GGUF_MAGIC {
        return Err("Missing GGUF magic bytes (file may be corrupt or misnamed).".to_string());
    }
    Ok(())
}

/// Non-recursive list of validated `.gguf` filenames in `chat_models_dir` (typically `…/models/chat`).
pub fn list_sorted_validated_gguf_filenames(chat_models_dir: &Path) -> Result<Vec<String>, String> {
    if !chat_models_dir.exists() {
        return Ok(Vec::new());
    }
    let mut names: Vec<String> = std::fs::read_dir(chat_models_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter(|e| {
            let fname = e.file_name();
            let name = fname.to_string_lossy();
            !name.starts_with('.') && name.to_ascii_lowercase().ends_with(".gguf")
        })
        .filter(|e| validate_gguf_file(&e.path()).is_ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    Ok(names)
}

/// Spawn `llama-server` (or a compatible binary) from `ATHENA_LLAMA_SERVER_BIN`, bound to `127.0.0.1`
/// on an ephemeral port. Any previous managed child is replaced. Returns the chosen TCP port.
pub fn try_spawn_llama_server_localhost(model_gguf_path: &Path) -> Result<u16, String> {
    let bin = std::env::var("ATHENA_LLAMA_SERVER_BIN")
        .map_err(|_| "ATHENA_LLAMA_SERVER_BIN is not set.".to_string())?;
    let bin = bin.trim();
    if bin.is_empty() {
        return Err("ATHENA_LLAMA_SERVER_BIN is empty.".to_string());
    }
    validate_gguf_file(model_gguf_path)?;

    shutdown_managed_local_inference();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    drop(listener);

    let model_str = model_gguf_path
        .to_str()
        .ok_or_else(|| "Model path is not valid UTF-8.".to_string())?;

    let mut cmd = Command::new(bin);
    cmd.args([
        "--model",
        model_str,
        "--host",
        "127.0.0.1",
        "--port",
        &port.to_string(),
    ]);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn local inference server: {e}"))?;

    let mut slot = child_slot().lock().map_err(|e| e.to_string())?;
    *slot = Some(child);
    set_managed_port(Some(port));
    Ok(port)
}
