//! ONNX cross-encoder reranking (query + passage pairs) on CPU.
//! Expects `model.onnx` + `tokenizer.json` under AppData `models/rerankers/...` or bundled resources.
//! If files are missing, uses a deterministic stub so the app still runs.

use ndarray::Array2;
use ort::execution_providers::CPUExecutionProvider;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::{Session, SessionInputValue};
use ort::value::Tensor;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

static GLOBAL: OnceLock<RwLock<Arc<Reranker>>> = OnceLock::new();

pub fn init_global(reranker: Arc<Reranker>) -> Result<(), String> {
    GLOBAL
        .set(RwLock::new(reranker))
        .map_err(|_| "Reranker global already initialized".to_string())
}

pub fn replace_global(reranker: Arc<Reranker>) {
    if let Some(lock) = GLOBAL.get() {
        if let Ok(mut w) = lock.write() {
            *w = reranker;
        }
    }
}

pub fn score_pairs_global(query: &str, passages: &[String]) -> Result<Vec<f32>, String> {
    if passages.is_empty() {
        return Ok(Vec::new());
    }
    let lock = GLOBAL
        .get()
        .ok_or_else(|| "Reranker not initialized".to_string())?;
    let g = lock.read().map_err(|e| e.to_string())?;
    g.score_pairs(query, passages)
}

#[derive(Debug)]
enum Inner {
    Ort {
        session: std::sync::Mutex<Session>,
        tokenizer: Tokenizer,
        wants_token_type_ids: bool,
    },
    Stub,
}

pub struct Reranker {
    inner: Inner,
}

impl Reranker {
    /// Prefer `app_data_reranker_dir` (AppData `models/rerankers/...`), then optional Tauri resource dir, then dev `resources/reranker`.
    pub fn load(app_data_reranker_dir: Option<&Path>, bundle_dir: Option<&Path>) -> Self {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(p) = app_data_reranker_dir {
            candidates.push(p.to_path_buf());
        }
        if let Some(p) = bundle_dir {
            candidates.push(p.to_path_buf());
        }
        candidates.push(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/reranker"),
        );
        for dir in candidates {
            let model = dir.join("model.onnx");
            let tok = dir.join("tokenizer.json");
            if model.is_file() && tok.is_file() {
                match Self::load_ort(&model, &tok) {
                    Ok(inner) => {
                        eprintln!(
                            "[athena] rerank: loaded ONNX cross-encoder from {}",
                            model.display()
                        );
                        return Self { inner };
                    }
                    Err(e) => {
                        eprintln!(
                            "[athena] rerank: failed to load {}: {e}",
                            model.display()
                        );
                    }
                }
            }
        }
        eprintln!(
            "[athena] rerank: ONNX not found; using stub scores (AppData models/rerankers or resources/reranker/)"
        );
        Self { inner: Inner::Stub }
    }

    fn load_ort(model_path: &Path, tokenizer_path: &Path) -> Result<Inner, String> {
        let tokenizer = Tokenizer::from_file(tokenizer_path).map_err(|e| e.to_string())?;
        let session = Session::builder()
            .map_err(|e| e.to_string())?
            .with_execution_providers([CPUExecutionProvider::default().build()])
            .map_err(|e| e.to_string())?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| e.to_string())?
            .commit_from_file(model_path)
            .map_err(|e| e.to_string())?;
        let wants_token_type_ids = session.inputs().iter().any(|o| {
            let n = o.name();
            n == "token_type_ids" || n.starts_with("token_type_ids")
        });
        Ok(Inner::Ort {
            session: std::sync::Mutex::new(session),
            tokenizer,
            wants_token_type_ids,
        })
    }

    pub fn score_pairs(&self, query: &str, passages: &[String]) -> Result<Vec<f32>, String> {
        let q = query.trim();
        let mut out = Vec::with_capacity(passages.len());
        match &self.inner {
            Inner::Stub => {
                for p in passages {
                    out.push(stub_score_pair(q, p.as_str()));
                }
            }
            Inner::Ort {
                session,
                tokenizer,
                wants_token_type_ids,
            } => {
                for p in passages {
                    out.push(ort_score_pair(
                        session,
                        tokenizer,
                        *wants_token_type_ids,
                        q,
                        p.as_str(),
                    )?);
                }
            }
        }
        Ok(out)
    }

    pub fn is_onnx_loaded(&self) -> bool {
        matches!(self.inner, Inner::Ort { .. })
    }
}

fn ort_score_pair(
    session: &std::sync::Mutex<Session>,
    tokenizer: &Tokenizer,
    wants_token_type_ids: bool,
    query: &str,
    passage: &str,
) -> Result<f32, String> {
    let mut tok = tokenizer.clone();
    let _ = tok.with_padding(Some(PaddingParams {
        strategy: tokenizers::PaddingStrategy::BatchLongest,
        direction: tokenizers::PaddingDirection::Right,
        pad_to_multiple_of: None,
        pad_id: 0,
        pad_type_id: 0,
        pad_token: "[PAD]".to_string(),
    }));
    let _ = tok.with_truncation(Some(TruncationParams {
        max_length: 512,
        strategy: tokenizers::TruncationStrategy::LongestFirst,
        stride: 0,
        direction: tokenizers::TruncationDirection::Right,
    }));

    let enc = tok
        .encode((query, passage), true)
        .map_err(|e| e.to_string())?;
    let ids_u32 = enc.get_ids();
    let mask_u32 = enc.get_attention_mask();
    let type_u32 = enc.get_type_ids();
    let seq = ids_u32.len();
    if seq == 0 {
        return Ok(0.0f32);
    }

    let ids_i64: Vec<i64> = ids_u32.iter().map(|&x| x as i64).collect();
    let mask_i64: Vec<i64> = mask_u32.iter().map(|&x| x as i64).collect();

    let input_ids = Array2::from_shape_vec((1, seq), ids_i64).map_err(|e| e.to_string())?;
    let attention_mask =
        Array2::from_shape_vec((1, seq), mask_i64.clone()).map_err(|e| e.to_string())?;

    let ids_tensor = Tensor::from_array(input_ids.into_dyn()).map_err(|e| e.to_string())?;
    let mask_tensor =
        Tensor::from_array(attention_mask.into_dyn()).map_err(|e| e.to_string())?;

    let mut inputs: Vec<(Cow<'static, str>, SessionInputValue<'_>)> = vec![
        (
            Cow::Borrowed("input_ids"),
            SessionInputValue::from(ids_tensor),
        ),
        (
            Cow::Borrowed("attention_mask"),
            SessionInputValue::from(mask_tensor),
        ),
    ];
    if wants_token_type_ids {
        let batch_size = 1usize;
        let type_i64: Vec<i64> = if type_u32.len() == seq {
            type_u32.iter().map(|&x| x as i64).collect()
        } else {
            vec![0i64; seq]
        };
        let token_type_ids = Array2::from_shape_vec((batch_size, seq), type_i64)
            .map_err(|e| e.to_string())?;
        let tt_tensor =
            Tensor::from_array(token_type_ids.into_dyn()).map_err(|e| e.to_string())?;
        inputs.push((
            Cow::Borrowed("token_type_ids"),
            SessionInputValue::from(tt_tensor),
        ));
    }

    let mut sess = session.lock().map_err(|e| e.to_string())?;
    let outputs = sess.run(inputs).map_err(|e| e.to_string())?;

    let value = outputs
        .iter()
        .next()
        .ok_or_else(|| "cross-encoder ONNX produced no outputs".to_string())?
        .1;
    let (shape, data) = value
        .try_extract_tensor::<f32>()
        .map_err(|e| e.to_string())?;
    Ok(logits_to_score(data, shape))
}

fn logits_to_score(data: &[f32], shape: &[i64]) -> f32 {
    if data.is_empty() {
        return 0.0;
    }
    match shape.len() {
        1 => data[0],
        2 => {
            let cols = shape.get(1).copied().unwrap_or(1);
            if cols <= 1 {
                data[0]
            } else {
                *data.last().unwrap_or(&data[0])
            }
        }
        _ => data[0],
    }
}

fn stub_score_pair(query: &str, passage: &str) -> f32 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    query.hash(&mut hasher);
    passage.hash(&mut hasher);
    let h = hasher.finish();
    (h as f32) / (u64::MAX as f32)
}
