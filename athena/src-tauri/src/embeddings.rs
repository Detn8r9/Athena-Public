//! Local deterministic embeddings: tokenize text, hash into a fixed-size vector,
//! L2-normalize. No external model or network; runs entirely in Rust.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub const EMBEDDING_DIM: usize = 256;

fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '\'' {
            current.push(c.to_lowercase().next().unwrap_or(c));
        } else {
            if current.len() >= 2 {
                out.push(current.clone());
            }
            current.clear();
        }
    }
    if current.len() >= 2 {
        out.push(current);
    }
    out
}

fn hash_to_bucket(token: &str, dim: usize) -> usize {
    let mut hasher = DefaultHasher::new();
    token.hash(&mut hasher);
    (hasher.finish() as usize) % dim
}

/// Build a unit-length embedding vector for the given text. Deterministic and offline.
pub fn embed_text(text: &str) -> Vec<f32> {
    let mut vec = vec![0.0f32; EMBEDDING_DIM];
    for token in tokenize(text) {
        let b = hash_to_bucket(&token, EMBEDDING_DIM);
        vec[b] += 1.0;
    }
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut vec {
            *x /= norm;
        }
    }
    vec
}

/// Cosine similarity of two L2-normalized vectors. Returns a value in [-1, 1] (1 = identical).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}
