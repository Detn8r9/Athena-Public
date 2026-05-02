//! Async HTTP downloads for bootstrapping ONNX assets into app data (non-blocking for the UI thread).

use crate::config::ModelDirectoryPaths;
use futures_util::StreamExt;
use std::path::Path;
use tokio::io::AsyncWriteExt;

const EMBED_MODEL_ONNX_URL: &str =
    "https://huggingface.co/onnx-models/all-MiniLM-L6-v2-onnx/resolve/main/model.onnx";
const EMBED_TOKENIZER_URL: &str =
    "https://huggingface.co/onnx-models/all-MiniLM-L6-v2-onnx/resolve/main/tokenizer.json";

const RERANK_MODEL_ONNX_URL: &str =
    "https://huggingface.co/cross-encoder/ms-marco-MiniLM-L-6-v2/resolve/main/onnx/model.onnx";
const RERANK_TOKENIZER_URL: &str =
    "https://huggingface.co/cross-encoder/ms-marco-MiniLM-L-6-v2/resolve/main/tokenizer.json";

fn hf_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("Athena/0.1 (Tauri; ONNX bootstrap)")
        .timeout(std::time::Duration::from_secs(7200))
        .build()
        .map_err(|e| e.to_string())
}

/// Streams a remote file to `dest` using a temp file and atomic rename.
pub async fn download_url_to_file(url: &str, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    let tmp = dest.with_extension({
        let ext = dest
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("bin");
        format!("{ext}.part")
    });

    let client = hf_client()?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!(
            "download failed HTTP {} for {}",
            resp.status(),
            url
        ));
    }

    let mut stream = resp.bytes_stream();
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .map_err(|e| e.to_string())?;

    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);

    tokio::fs::rename(&tmp, dest)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Downloads default embedding and reranker ONNX assets when `model.onnx` / `tokenizer.json` are missing.
pub async fn bootstrap_models(paths: &ModelDirectoryPaths) -> Result<(), String> {
    let emb_dir = &paths.embedding_all_minilm;
    let emb_model = emb_dir.join("model.onnx");
    let emb_tok = emb_dir.join("tokenizer.json");
    if !emb_model.is_file() {
        eprintln!(
            "[athena] downloading embedding model.onnx → {}",
            emb_model.display()
        );
        download_url_to_file(EMBED_MODEL_ONNX_URL, &emb_model).await?;
    }
    if !emb_tok.is_file() {
        eprintln!(
            "[athena] downloading embedding tokenizer.json → {}",
            emb_tok.display()
        );
        download_url_to_file(EMBED_TOKENIZER_URL, &emb_tok).await?;
    }

    let rr_dir = &paths.reranker_ms_marco;
    let rr_model = rr_dir.join("model.onnx");
    let rr_tok = rr_dir.join("tokenizer.json");
    if !rr_model.is_file() {
        eprintln!(
            "[athena] downloading reranker model.onnx → {}",
            rr_model.display()
        );
        download_url_to_file(RERANK_MODEL_ONNX_URL, &rr_model).await?;
    }
    if !rr_tok.is_file() {
        eprintln!(
            "[athena] downloading reranker tokenizer.json → {}",
            rr_tok.display()
        );
        download_url_to_file(RERANK_TOKENIZER_URL, &rr_tok).await?;
    }

    eprintln!("[athena] ONNX model bootstrap finished.");
    Ok(())
}
