//! Bounded reads for RAG ingestion so multi-gigabyte text dumps never load fully into RAM.
//! Plain-text formats use a hard byte cap; binary formats keep using format-specific readers.

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Default maximum UTF-8 payload size for a single RAG ingest (plain text / CSV / MD).
pub const MAX_RAG_PLAIN_TEXT_BYTES: usize = 48 * 1024 * 1024;

/// Read entire file as UTF-8, rejecting if it exceeds `cap` bytes on disk.
pub fn read_utf8_file_capped(path: &Path, cap: usize) -> Result<String, String> {
    let f = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut buf = Vec::new();
    f.take(cap.saturating_add(1) as u64)
        .read_to_end(&mut buf)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    if buf.len() > cap {
        return Err(format!(
            "Document exceeds {} byte RAG ingest cap ({}). Split the file or raise the cap.",
            cap,
            path.display()
        ));
    }
    String::from_utf8(buf).map_err(|e| format!("{} is not valid UTF-8: {e}", path.display()))
}
