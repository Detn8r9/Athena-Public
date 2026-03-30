//! ONNX sentence embeddings (all-MiniLM-L6-v2-class models, 384 dimensions) on CPU.
//! Expects `model.onnx` + `tokenizer.json` under AppData `models/embeddings/...` or bundled resources.
//! If files are missing, falls back to a deterministic stub so the app still runs.

use ndarray::{Array2, ArrayView2, ArrayView3};
use ort::execution_providers::CPUExecutionProvider;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::{Session, SessionInputValue};
use ort::value::Tensor;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

pub const NEURAL_EMBEDDING_DIM: usize = 384;

static GLOBAL: OnceLock<RwLock<Arc<NeuralEmbedder>>> = OnceLock::new();

pub fn init_global(embedder: Arc<NeuralEmbedder>) -> Result<(), String> {
    GLOBAL
        .set(RwLock::new(embedder))
        .map_err(|_| "NeuralEmbedder global already initialized".to_string())
}

pub fn replace_global(embedder: Arc<NeuralEmbedder>) {
    if let Some(lock) = GLOBAL.get() {
        if let Ok(mut w) = lock.write() {
            *w = embedder;
        }
    }
}

pub fn embed_query(text: &str) -> Result<Vec<f32>, String> {
    let lock = GLOBAL
        .get()
        .ok_or_else(|| "NeuralEmbedder not initialized".to_string())?;
    let g = lock.read().map_err(|e| e.to_string())?;
    g.embed(text)
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

pub struct NeuralEmbedder {
    inner: Inner,
}

impl NeuralEmbedder {
    /// Prefer `app_data_embedding_dir` (AppData `models/embeddings/...`), then optional Tauri resource dir, then dev `resources/embedding`.
    pub fn load(app_data_embedding_dir: Option<&Path>, bundle_dir: Option<&Path>) -> Self {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(p) = app_data_embedding_dir {
            candidates.push(p.to_path_buf());
        }
        if let Some(p) = bundle_dir {
            candidates.push(p.to_path_buf());
        }
        candidates.push(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/embedding"),
        );
        for dir in candidates {
            let model = dir.join("model.onnx");
            let tok = dir.join("tokenizer.json");
            if model.is_file() && tok.is_file() {
                match Self::load_ort(&model, &tok) {
                    Ok(inner) => {
                        eprintln!(
                            "[athena] neural_embed: loaded ONNX from {}",
                            model.display()
                        );
                        return Self { inner };
                    }
                    Err(e) => {
                        eprintln!(
                            "[athena] neural_embed: failed to load {}: {e}",
                            model.display()
                        );
                    }
                }
            }
        }
        eprintln!(
            "[athena] neural_embed: ONNX not found; using stub {}-dim embeddings (AppData models/embeddings or resources/embedding/)",
            NEURAL_EMBEDDING_DIM
        );
        Self {
            inner: Inner::Stub,
        }
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

    pub fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let t = text.trim();
        if t.is_empty() {
            return Ok(vec![0.0f32; NEURAL_EMBEDDING_DIM]);
        }
        match &self.inner {
            Inner::Stub => Ok(stub_embed_384(t)),
            Inner::Ort {
                session,
                tokenizer,
                wants_token_type_ids,
            } => ort_embed(session, tokenizer, *wants_token_type_ids, t),
        }
    }

    pub fn is_onnx_loaded(&self) -> bool {
        matches!(self.inner, Inner::Ort { .. })
    }
}

fn ort_embed(
    session: &std::sync::Mutex<Session>,
    tokenizer: &Tokenizer,
    wants_token_type_ids: bool,
    text: &str,
) -> Result<Vec<f32>, String> {
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
        max_length: 128,
        strategy: tokenizers::TruncationStrategy::LongestFirst,
        stride: 0,
        direction: tokenizers::TruncationDirection::Right,
    }));

    let enc = tok.encode(text, true).map_err(|e| e.to_string())?;
    let ids_u32 = enc.get_ids();
    let mask_u32 = enc.get_attention_mask();
    let type_u32 = enc.get_type_ids();
    let seq = ids_u32.len();
    if seq == 0 {
        return Ok(vec![0.0f32; NEURAL_EMBEDDING_DIM]);
    }

    let ids_i64: Vec<i64> = ids_u32.iter().map(|&x| x as i64).collect();
    let mask_i64: Vec<i64> = mask_u32.iter().map(|&x| x as i64).collect();

    let input_ids = Array2::from_shape_vec((1, seq), ids_i64).map_err(|e| e.to_string())?;
    let attention_mask = Array2::from_shape_vec((1, seq), mask_i64.clone()).map_err(|e| e.to_string())?;

    let token_type_ids: Option<Array2<i64>> = if wants_token_type_ids {
        Some(if type_u32.len() == seq {
            let type_i64: Vec<i64> = type_u32.iter().map(|&x| x as i64).collect();
            Array2::from_shape_vec((1, seq), type_i64).map_err(|e| e.to_string())?
        } else {
            Array2::<i64>::zeros((1, seq))
        })
    } else {
        None
    };

    println!(
        "[athena] Tensor shapes - input_ids: {:?}, mask: {:?}, token_type: {:?}",
        input_ids.shape(),
        attention_mask.shape(),
        token_type_ids.as_ref().map(|t| t.shape())
    );

    let ids_tensor = Tensor::from_array(input_ids.into_dyn()).map_err(|e| e.to_string())?;
    let mask_tensor = Tensor::from_array(attention_mask.into_dyn()).map_err(|e| e.to_string())?;

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
    if let Some(tt) = token_type_ids {
        let tt_tensor = Tensor::from_array(tt.into_dyn()).map_err(|e| e.to_string())?;
        inputs.push((
            Cow::Borrowed("token_type_ids"),
            SessionInputValue::from(tt_tensor),
        ));
    }

    let mut sess = session.lock().map_err(|e| e.to_string())?;

    let outputs = match sess.run(inputs) {
        Ok(o) => o,
        Err(err) => {
            eprintln!("[athena] ONNX Execution Error: {:?}", err);
            return Err(err.to_string());
        }
    };

    let value = &outputs[0];
    let (shape, data) = value
        .try_extract_tensor::<f32>()
        .map_err(|e| e.to_string())?;

    let rank = shape.len();
    let out = if rank == 2 {
        let rows = shape[0] as usize;
        let cols = shape[1] as usize;
        if rows != 1 {
            return Err(format!("expected batch 1, got {rows}"));
        }
        if cols != NEURAL_EMBEDDING_DIM {
            return Err(format!("expected dim {NEURAL_EMBEDDING_DIM}, got {cols}"));
        }
        data.to_vec()
    } else if rank == 3 {
        let batch = shape[0] as usize;
        let s = shape[1] as usize;
        let dim = shape[2] as usize;
        if batch != 1 {
            return Err(format!("expected batch 1, got {batch}"));
        }
        let arr =
            ArrayView3::from_shape((batch, s, dim), data).map_err(|e| e.to_string())?;
        let m2 = Array2::from_shape_vec((1, seq), mask_i64).map_err(|e| e.to_string())?;
        mean_pool(arr, m2.view())
    } else {
        return Err(format!("unexpected ONNX output rank {rank}"));
    };

    if out.len() != NEURAL_EMBEDDING_DIM {
        return Err(format!(
            "embedding length {} != {}",
            out.len(),
            NEURAL_EMBEDDING_DIM
        ));
    }

    Ok(l2_normalize(out))
}

fn mean_pool(hidden: ArrayView3<f32>, mask: ArrayView2<i64>) -> Vec<f32> {
    let (_, seq, dim) = hidden.dim();
    let mut sum = vec![0.0f32; dim];
    let mut cnt = 0.0f32;
    for t in 0..seq {
        if mask[[0, t]] == 0 {
            continue;
        }
        for h in 0..dim {
            sum[h] += hidden[[0, t, h]];
        }
        cnt += 1.0;
    }
    if cnt > 0.0 {
        for x in &mut sum {
            *x /= cnt;
        }
    }
    sum
}

fn l2_normalize(mut v: Vec<f32>) -> Vec<f32> {
    let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 1e-8 {
        for x in &mut v {
            *x /= n;
        }
    }
    v
}

fn stub_embed_384(text: &str) -> Vec<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut v = vec![0.0f32; NEURAL_EMBEDDING_DIM];
    for token in stub_tokenize(text) {
        let mut hasher = DefaultHasher::new();
        token.hash(&mut hasher);
        let b = (hasher.finish() as usize) % NEURAL_EMBEDDING_DIM;
        v[b] += 1.0;
    }
    l2_normalize(v)
}

fn stub_tokenize(text: &str) -> Vec<String> {
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
