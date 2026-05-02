//! Document chunking for RAG: bounded UTF-8 reads plus semantic parent/child windows.
//! Call from `tokio::task::spawn_blocking` when ingesting large extracted library files.

use std::path::Path;

use crate::chunking::{self, ParentBlock};
use crate::document_rag_read;

/// Read a UTF-8 text-like document with a hard byte cap, then build the parent/child chunk plan.
pub fn chunk_plain_document_for_rag(path: &Path, document_title: &str) -> Result<(String, Vec<ParentBlock>), String> {
    let text = document_rag_read::read_utf8_file_capped(path, document_rag_read::MAX_RAG_PLAIN_TEXT_BYTES)?;
    let words = chunking::words_from_plain_text(&text);
    let plan = chunking::build_parent_child_plan(&words, document_title);
    Ok((text, plan))
}
