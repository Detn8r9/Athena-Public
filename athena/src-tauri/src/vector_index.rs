//! Pipeline: full document text → semantic chunks → local embeddings → vector DB.
//! Call index_document when a file is ingested so retrieval can use similarity search.

use crate::chunking;
use crate::db::Database;
use crate::embeddings;

/// Chunk the document, embed each chunk, and persist to the vector store.
/// Removes any existing embeddings for this file_path before inserting.
pub fn index_document(db: &Database, file_path: &str, full_text: &str) -> Result<(), String> {
    db.delete_embeddings_for_file(file_path)?;
    let chunks = chunking::chunk_text_default(full_text);
    for chunk in chunks {
        let vector = embeddings::embed_text(&chunk.text);
        db.upsert_chunk_embedding(file_path, chunk.index as i64, &chunk.text, &vector)?;
    }
    Ok(())
}
