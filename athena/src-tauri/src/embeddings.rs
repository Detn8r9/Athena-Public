//! Cosine similarity helpers. Embedding dimension matches ONNX sentence models (384).

pub use crate::neural_embed::NEURAL_EMBEDDING_DIM as EMBEDDING_DIM;

/// Embed text using the global neural model (ONNX or stub).
pub fn embed_text(text: &str) -> Vec<f32> {
    crate::neural_embed::embed_query(text).unwrap_or_else(|e| {
        eprintln!("[athena] embed_text: {e}");
        vec![0.0f32; EMBEDDING_DIM]
    })
}

/// Cosine similarity of two L2-normalized vectors. Returns a value in [-1, 1] (1 = identical).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}
