//! Retrieval regression tests that need the real ONNX embedding and reranker models.
//!
//! They are `#[ignore]`d so `cargo test` stays hermetic. Run them with:
//!
//! ```bash
//! cargo test --lib library_eval -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Models are looked up in `ATHENA_EVAL_MODELS` (a directory holding `embeddings/<name>/` and
//! `rerankers/<name>/`), falling back to the default AthenaData location for this OS user.

use crate::db::Database;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn models_root() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ATHENA_EVAL_MODELS") {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var("HOME").ok()?;
    let p = Path::new(&home).join("Library/Application Support/com.athena.micro/user/models");
    p.is_dir().then_some(p)
}

fn first_model_dir(parent: &Path) -> Option<PathBuf> {
    std::fs::read_dir(parent)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.join("model.onnx").is_file() && p.join("tokenizer.json").is_file())
}

/// Load the real models into the process-wide slots. Returns false when they are not installed.
fn load_real_models() -> bool {
    let Some(root) = models_root() else {
        return false;
    };
    let (Some(emb_dir), Some(rr_dir)) = (
        first_model_dir(&root.join("embeddings")),
        first_model_dir(&root.join("rerankers")),
    ) else {
        return false;
    };
    let emb = Arc::new(crate::neural_embed::NeuralEmbedder::load(Some(&emb_dir), None));
    let rr = Arc::new(crate::rerank::Reranker::load(Some(&rr_dir), None));
    if !emb.is_onnx_loaded() || !rr.is_onnx_loaded() {
        return false;
    }
    if crate::neural_embed::init_global(Arc::clone(&emb)).is_err() {
        crate::neural_embed::replace_global(emb);
    }
    if crate::rerank::init_global(Arc::clone(&rr)).is_err() {
        crate::rerank::replace_global(rr);
    }
    true
}

const WATER: &str = "Field water purification. Surface water must be treated before drinking. \
Bring water to a rolling boil for one minute; above 2,000 metres boil for three minutes. \
If you cannot boil, add two drops of unscented household bleach per litre, stir, and wait thirty \
minutes. Cloudy water should be filtered through cloth first and the bleach dose doubled. \
Iodine tablets are an alternative: one tablet per litre, wait thirty minutes.";

const GENERATOR: &str = "Generator maintenance schedule for the Kestrel 3500 portable generator. \
Change the engine oil after the first 20 hours of operation and every 100 hours after that. \
Use SAE 10W-30 oil; the crankcase holds 0.6 litres. Clean the air filter every 50 hours. \
Replace the spark plug (type BPR6ES, gap 0.7 mm) every 300 hours. Never refuel while the engine \
is running or hot.";

const RADIO: &str = "Radio check-in procedure. Each outpost checks in on the primary channel at \
0700 and 1900 local time. The net control station calls the roll in alphabetical order. \
Reply with your call sign, your status (green, amber or red) and any traffic. If you miss two \
consecutive check-ins, the net control station dispatches a welfare patrol.";

fn fixture_db(dir: &Path) -> Database {
    let db = Database::new(&dir.join("eval.db")).expect("open db");
    for (name, title, text) in [
        ("water.txt", "Water Purification", WATER),
        ("generator.txt", "Generator Manual", GENERATOR),
        ("radio.txt", "Radio SOP", RADIO),
    ] {
        let path = format!("/library/{name}");
        db.upsert_file(&path, "text", name, None, None, Some(text), None, Some(title), None, None)
            .expect("upsert file");
        crate::vector_index::index_document(&db, &path, title, None, text).expect("index");
    }
    db
}

#[test]
#[ignore = "needs the ONNX embedding and reranker models"]
fn fixture_questions_retrieve_the_right_document() {
    if !load_real_models() {
        eprintln!("skipped: models not installed");
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let db = fixture_db(dir.path());

    let cases = [
        ("how long do I boil water to make it safe to drink?", "water.txt"),
        ("how much bleach per litre?", "water.txt"),
        ("what oil does the generator take?", "generator.txt"),
        ("how often should I replace the spark plug?", "generator.txt"),
        ("when are the daily radio check-ins?", "radio.txt"),
        ("what happens if an outpost misses two check-ins?", "radio.txt"),
    ];
    for (question, expected) in cases {
        let hits = db.hybrid_search(question).expect("hybrid_search");
        let top = hits.first().unwrap_or_else(|| panic!("no hits for {question:?}"));
        eprintln!("{question:?} -> {} (score {:.2})", top.file_path, top.score);
        assert!(
            top.file_path.ends_with(expected),
            "{question:?}: expected {expected}, got {}",
            top.file_path
        );
    }
}

#[test]
#[ignore = "needs the ONNX embedding and reranker models"]
fn unrelated_questions_retrieve_nothing() {
    if !load_real_models() {
        eprintln!("skipped: models not installed");
        return;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let db = fixture_db(dir.path());
    for question in [
        "who won the 1998 football world cup?",
        "write me a poem about autumn leaves",
        "what is the boiling point of mercury on Venus?",
    ] {
        let scored = db.hybrid_search_scored(question).expect("hybrid_search_scored");
        for h in &scored {
            eprintln!("{question:?} -> {} score {:.2} relevant={}", h.hit.file_path, h.hit.score, h.relevant);
        }
        let hits = db.hybrid_search(question).expect("hybrid_search");
        assert!(
            hits.is_empty(),
            "{question:?} should retrieve nothing, got {:?}",
            hits.iter().map(|h| (&h.file_path, h.score)).collect::<Vec<_>>()
        );
    }
}

/// Diagnostic, not an assertion: print what retrieval returns for questions against a real index.
///
/// `ATHENA_EVAL_DB` = path to a *copy* of `athena_index.db`; `ATHENA_EVAL_QUERIES` = questions
/// separated by `|`.
#[test]
#[ignore = "diagnostic: needs ATHENA_EVAL_DB and the ONNX models"]
fn print_retrieval_for_local_index() {
    let (Ok(db_path), Ok(queries)) = (
        std::env::var("ATHENA_EVAL_DB"),
        std::env::var("ATHENA_EVAL_QUERIES"),
    ) else {
        eprintln!("skipped: set ATHENA_EVAL_DB and ATHENA_EVAL_QUERIES");
        return;
    };
    if !load_real_models() {
        eprintln!("skipped: models not installed");
        return;
    }
    let db = Database::new(Path::new(&db_path)).expect("open db");
    for q in queries.split('|').map(str::trim).filter(|q| !q.is_empty()) {
        let started = std::time::Instant::now();
        let scored = db.hybrid_search_scored(q).expect("hybrid_search_scored");
        eprintln!("\n=== {q}  ({} ms, {} candidates)", started.elapsed().as_millis(), scored.len());
        for h in scored.iter().take(10) {
            let name = Path::new(&h.hit.file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?");
            let preview: String = h.hit.expanded_text.chars().take(70).collect();
            eprintln!(
                "  {} {:>7.2}  {:<34} p{:<4} {}",
                if h.relevant { "KEEP" } else { "drop" },
                h.hit.score,
                name.chars().take(34).collect::<String>(),
                h.hit.page_number.map(|p| p.to_string()).unwrap_or_else(|| "-".into()),
                preview.replace('\n', " ")
            );
        }
        let kept = scored.iter().filter(|h| h.relevant).count();
        let hits: Vec<_> = scored.into_iter().filter(|h| h.relevant).map(|h| h.hit).collect();
        let budget = crate::library_context::library_context_char_budget(Some(4096), q.len());
        let assembled = crate::library_context::assemble_library_context(&hits, budget);
        eprintln!(
            "  kept {kept}; prompt gets {} block(s), {} chars of {budget}; sources: {:?}",
            assembled.blocks,
            assembled.body.len(),
            assembled.sources.iter().map(|s| (&s.display_title, s.page_number)).collect::<Vec<_>>()
        );
    }
}
