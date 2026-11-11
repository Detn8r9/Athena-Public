//! Phase 1: mock Ollama HTTP stream that sends one NDJSON line then hangs (no more bytes).
//! Verifies `ollama_stream_read_lines` idle timeout returns an error instead of blocking forever.

use athena_lib::{chat_stream, ChatMessage};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

/// Serialize tests that mutate Ollama-related env vars (shared process environment).
static OLLAMA_ENV_LOCK: Mutex<()> = Mutex::new(());

/// Minimal HTTP/1.1 chunked response: one chunk of Ollama-style JSON, then the connection stalls
/// (no terminating `0\\r\\n\\r\\n`), so the client waits indefinitely for more body bytes.
fn spawn_chunked_stall_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock Ollama");
    let port = listener.local_addr().expect("addr").port();
    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buf = Vec::new();
        let mut tmp = [0u8; 4096];
        loop {
            let n = match stream.read(&mut tmp) {
                Ok(n) => n,
                Err(_) => return,
            };
            if n == 0 {
                return;
            }
            buf.extend_from_slice(&tmp[..n]);
            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }

        let line = "{\"message\":{\"content\":\"partial\"},\"done\":false}\n";
        let mut response = String::new();
        response.push_str("HTTP/1.1 200 OK\r\n");
        response.push_str("Transfer-Encoding: chunked\r\n");
        response.push_str("Content-Type: application/x-ndjson\r\n");
        response.push_str("\r\n");
        response.push_str(&format!("{:x}\r\n", line.len()));
        response.push_str(line);
        response.push_str("\r\n");
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
        // Intentionally never send the closing chunked terminator; stall until the client times out.
        thread::sleep(Duration::from_secs(180));
    });
    port
}

struct OllamaTestEnvGuard;

impl Drop for OllamaTestEnvGuard {
    fn drop(&mut self) {
        std::env::remove_var("ATHENA_OLLAMA_BASE_URL");
        std::env::remove_var("ATHENA_OLLAMA_STREAM_IDLE_MS");
        std::env::remove_var("ATHENA_OLLAMA_STREAM_MAX_DURATION_MS");
    }
}

#[test]
fn chat_stream_errors_when_body_stalls_after_first_chunk() {
    let _env = OLLAMA_ENV_LOCK.lock().expect("ollama env lock");

    let port = spawn_chunked_stall_server();
    thread::sleep(Duration::from_millis(50));

    std::env::set_var("ATHENA_OLLAMA_BASE_URL", format!("http://127.0.0.1:{port}"));
    // Short idle window so the test finishes quickly; production default remains 30s via env unset.
    std::env::set_var("ATHENA_OLLAMA_STREAM_IDLE_MS", "100");
    std::env::set_var("ATHENA_OLLAMA_STREAM_MAX_DURATION_MS", "60000");
    let _guard = OllamaTestEnvGuard;

    let mut aggregated = String::new();
    let result = chat_stream(
        None,
        "test-model",
        vec![ChatMessage {
            role: "user".to_string(),
            content: "ping".to_string(),
        images: None,
        }],
        &mut |s| aggregated.push_str(s),
        || {},
        None,
        None,
    );

    assert!(
        result.is_err(),
        "expected idle timeout error, got Ok with output {aggregated:?}"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("stalled") || err.contains("Ollama stream"),
        "unexpected error message: {err}"
    );
}
