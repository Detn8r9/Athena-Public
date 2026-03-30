//! Pipeline: full document text → parent/child chunks → local embeddings → vector DB.
//! Parents carry ~1000-word context; children (~250 words) are embedded for retrieval.

use crate::chunking::{
    build_parent_child_plan, words_from_page_map, words_from_plain_text, WordPage,
};
use crate::db::Database;
use crate::embeddings;

/// Index a library document: delete prior rows, insert parent rows (zero vector, no vec index)
/// and child rows (embedding of `retrieval_text`, sqlite-vec row).
pub fn index_document(
    db: &Database,
    file_path: &str,
    document_title: &str,
    index_pages: Option<&[(u32, String)]>,
    full_text: &str,
) -> Result<(), String> {
    println!("[athena] Checkpoint V1: index_document entry for {}", file_path);
    println!(
        "[athena] Chunking and embedding for file: {}",
        file_path
    );
    println!("[athena] Checkpoint V2: deleting prior embeddings for path");
    db.delete_embeddings_for_file(file_path).map_err(|e| {
        eprintln!(
            "[athena] ingestion error: could not delete prior embeddings for {file_path}: {e}"
        );
        e
    })?;
    println!("[athena] Checkpoint V3: building WordPage list from text / page map");
    let words: Vec<WordPage> = if let Some(pages) = index_pages {
        if pages.is_empty() {
            words_from_plain_text(full_text)
        } else {
            words_from_page_map(pages)
        }
    } else {
        words_from_plain_text(full_text)
    };
    println!(
        "[athena] Checkpoint V4: WordPage count = {} (full_text len {} bytes)",
        words.len(),
        full_text.len()
    );
    println!(
        "[athena] Extracted text length: {} bytes. Starting chunker...",
        full_text.len()
    );
    println!(
        "[athena] Checkpoint V5: calling build_parent_child_plan (title len {} bytes)",
        document_title.len()
    );
    let plan = build_parent_child_plan(&words, document_title);
    println!(
        "[athena] Checkpoint V6: build_parent_child_plan returned {} parent block(s)",
        plan.len()
    );
    let chunks_for_embed: usize = plan.iter().map(|p| p.children.len()).sum();
    println!(
        "[athena] Generated {} chunks for embedding",
        chunks_for_embed
    );
    if plan.is_empty() {
        println!(
            "[athena] No chunkable segments for {}; skipping vector index.",
            file_path
        );
        return Ok(());
    }

    let parent_count = plan.len();
    let total_child_chunks: usize = plan.iter().map(|p| p.children.len()).sum();
    println!(
        "[athena] Inserting {} parent block(s), {} child chunk(s) to embed for {}",
        parent_count, total_child_chunks, file_path
    );

    let zero = vec![0.0f32; embeddings::EMBEDDING_DIM];
    let mut chunk_index: i64 = 0;
    let mut embedded_child_i: usize = 0;
    for parent in plan {
        let pid = db
            .insert_chunk_embedding_row(
                file_path,
                chunk_index,
                Some("parent"),
                None,
                parent.page_start,
                parent.page_end,
                &parent.chunk_text,
                &parent.retrieval_text,
                &zero,
                false,
            )
            .map_err(|e| {
                eprintln!(
                    "[athena] ingestion error: failed to insert parent chunk (index {}) for {file_path}: {e}",
                    chunk_index
                );
                e
            })?;
        chunk_index += 1;
        for child in parent.children {
            let v = embeddings::embed_text(&child.retrieval_text);
            db.insert_chunk_embedding_row(
                file_path,
                chunk_index,
                Some("child"),
                Some(pid),
                child.page_start,
                child.page_end,
                &child.chunk_text,
                &child.retrieval_text,
                &v,
                true,
            )
            .map_err(|e| {
                eprintln!(
                    "[athena] ingestion error: failed to insert child chunk (index {}) for {file_path}: {e}",
                    chunk_index
                );
                e
            })?;
            chunk_index += 1;
            embedded_child_i += 1;
            if total_child_chunks > 0 {
                println!(
                    "[athena] Embedded chunk {}/{}",
                    embedded_child_i, total_child_chunks
                );
            }
        }
    }
    println!("[athena] Successfully ingested and saved to database!");
    Ok(())
}
