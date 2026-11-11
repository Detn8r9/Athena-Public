//! Turns reranked library hits into the excerpt block that goes into the chat system prompt.
//!
//! Three jobs, all of which used to be done implicitly (and wrongly) at the call sites:
//!
//! 1. **Clean query** — retrieval must run on what the user typed, not on the prompt the client
//!    wraps around it (mode instructions, radio logs, map context).
//! 2. **Context budget** — excerpts have to fit the model's context window alongside the rules,
//!    history and the answer. Anything past the window is silently truncated by the runtime, which
//!    looks to the user like "the assistant ignored my documents".
//! 3. **Honest sources** — the sources shown under a reply are exactly the documents whose text
//!    was put in front of the model.

use crate::db::HybridSearchHit;
use crate::StreamLibrarySource;

/// Context window assumed when the client does not send `num_ctx`.
pub const DEFAULT_NUM_CTX: u32 = 4096;

/// Conservative characters-per-token estimate for English prose with a typical BPE tokenizer.
const CHARS_PER_TOKEN: f32 = 3.5;
/// Tokens held back for the fixed parts of the system prompt (rules, persona, location, POIs).
const RESERVED_SYSTEM_TOKENS: usize = 900;
/// Tokens held back for the model's reply.
const RESERVED_ANSWER_TOKENS: usize = 700;
/// Never offer fewer characters than this: one full chunk should always fit.
const MIN_LIBRARY_CHARS: usize = 1_800;
/// Past this the excerpts stop helping and slow small local models down.
const MAX_LIBRARY_CHARS: usize = 24_000;
/// Sources listed under a reply.
const MAX_SOURCES: usize = 8;

/// Characters of library excerpts that fit, given the context window and what else is in it.
///
/// `other_prompt_chars` is everything already committed to the prompt besides the fixed system
/// sections: conversation history plus the current user message.
pub fn library_context_char_budget(num_ctx: Option<u32>, other_prompt_chars: usize) -> usize {
    let ctx_tokens = num_ctx.filter(|n| *n >= 512).unwrap_or(DEFAULT_NUM_CTX) as usize;
    let other_tokens = (other_prompt_chars as f32 / CHARS_PER_TOKEN).ceil() as usize;
    let free_tokens = ctx_tokens
        .saturating_sub(RESERVED_SYSTEM_TOKENS)
        .saturating_sub(RESERVED_ANSWER_TOKENS)
        .saturating_sub(other_tokens);
    let chars = (free_tokens as f32 * CHARS_PER_TOKEN) as usize;
    chars.clamp(MIN_LIBRARY_CHARS, MAX_LIBRARY_CHARS)
}

/// Recover the text the user actually typed from a client-wrapped prompt.
///
/// Current clients send the raw question separately (`retrieval_text`). This exists for older
/// clients and the LAN bridge, which only send the wrapped `message`.
pub fn strip_client_prompt_wrapping(message: &str) -> String {
    let mut t = message.trim();

    // "Use the following document context … Question: <text>" wrapper.
    const QUESTION_MARK: &str = "\n\nQuestion: ";
    if t.starts_with("Use the following document context")
        || t.contains("\n\nRecent Radio Traffic Context: ")
    {
        if let Some(idx) = t.rfind(QUESTION_MARK) {
            t = t[idx + QUESTION_MARK.len()..].trim();
        }
    }

    // Leading "[Quick mode] …\n\n" style instruction paragraph(s).
    loop {
        let lower = t.to_ascii_lowercase();
        let is_mode_prefix = [
            "[quick mode]",
            "[quick voice mode]",
            "[deep technical mode]",
            "[deep voice mode]",
            "[live voice call]",
        ]
        .iter()
        .any(|p| lower.starts_with(p));
        if !is_mode_prefix {
            break;
        }
        match t.find("\n\n") {
            Some(idx) => t = t[idx + 2..].trim_start(),
            None => break,
        }
        // The wrapper can sit inside the mode prefix.
        if t.starts_with("Use the following document context") {
            if let Some(idx) = t.rfind(QUESTION_MARK) {
                t = t[idx + QUESTION_MARK.len()..].trim();
            }
        }
    }

    // Trailing client notes about attachments / camera frames.
    for marker in [
        "\n\n[Background attached automatically by ATHENA.",
        "\n\n[A local camera or image frame is attached.",
        "\n\n[Attached files for this question:",
        "\n\n---\nLocal camera / image vision context",
        "\n\n[Camera frame was provided but local vision failed",
    ] {
        if let Some(idx) = t.find(marker) {
            t = t[..idx].trim_end();
        }
    }

    t.trim().to_string()
}

/// Library excerpts ready for the system prompt, plus the sources they came from.
#[derive(Debug, Default)]
pub struct AssembledLibraryContext {
    /// `--- Source N: Title (page P) ---` blocks. Empty when no hit fit or none was relevant.
    pub body: String,
    /// One entry per document that contributed text, in the order the model sees them.
    pub sources: Vec<StreamLibrarySource>,
    /// Number of excerpt blocks in `body`.
    pub blocks: usize,
}

struct Block {
    title: String,
    file_path: String,
    page: Option<i64>,
    timestamp: Option<f64>,
    parent_id: Option<i64>,
    parent_text: Option<String>,
    text: String,
}

fn title_for(hit: &HybridSearchHit) -> String {
    hit.display_title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            std::path::Path::new(&hit.file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Library document")
                .to_string()
        })
}

fn truncate_at_char_boundary(s: &str, max_chars: usize) -> &str {
    if s.len() <= max_chars {
        return s;
    }
    let mut end = max_chars;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Select hits in relevance order until `char_budget` is used, then widen the best hits to their
/// surrounding section where there is room.
///
/// `hits` must already be relevance-filtered and sorted best first (see `Database::hybrid_search`).
pub fn assemble_library_context(
    hits: &[HybridSearchHit],
    char_budget: usize,
) -> AssembledLibraryContext {
    // Per-block header overhead ("--- Source N: … ---" plus blank lines).
    const HEADER_OVERHEAD: usize = 60;

    let mut blocks: Vec<Block> = Vec::new();
    let mut used = 0usize;
    for hit in hits {
        let text = hit.expanded_text.trim();
        if text.is_empty() {
            continue;
        }
        let cost = text.len() + HEADER_OVERHEAD;
        if used + cost > char_budget {
            if blocks.is_empty() {
                // Always give the model the best hit, cut to fit.
                let room = char_budget.saturating_sub(HEADER_OVERHEAD).max(400);
                let cut = truncate_at_char_boundary(text, room);
                used += cut.len() + HEADER_OVERHEAD;
                blocks.push(Block {
                    title: title_for(hit),
                    file_path: hit.file_path.clone(),
                    page: hit.page_number,
                    timestamp: hit.timestamp_start,
                    parent_id: None,
                    parent_text: None,
                    text: cut.to_string(),
                });
            }
            // Lower-ranked hits are less relevant; do not skip ahead looking for a small one.
            break;
        }
        used += cost;
        blocks.push(Block {
            title: title_for(hit),
            file_path: hit.file_path.clone(),
            page: hit.page_number,
            timestamp: hit.timestamp_start,
            parent_id: hit.parent_id,
            parent_text: hit.parent_text.clone(),
            text: text.to_string(),
        });
    }

    // Parent expansion: replace the children of one section with the whole section when it fits.
    // Best-ranked sections get first claim on the remaining room.
    let mut i = 0;
    while i < blocks.len() {
        let (Some(pid), Some(parent)) = (blocks[i].parent_id, blocks[i].parent_text.clone()) else {
            i += 1;
            continue;
        };
        let parent = parent.trim().to_string();
        let sibling_chars: usize = blocks
            .iter()
            .filter(|b| b.parent_id == Some(pid))
            .map(|b| b.text.len() + HEADER_OVERHEAD)
            .sum();
        let new_cost = parent.len() + HEADER_OVERHEAD;
        if !parent.is_empty() && used - sibling_chars + new_cost <= char_budget {
            used = used - sibling_chars + new_cost;
            blocks[i].text = parent;
            blocks[i].parent_id = None;
            blocks[i].parent_text = None;
            let mut j = blocks.len();
            while j > i + 1 {
                j -= 1;
                if blocks[j].parent_id == Some(pid) {
                    blocks.remove(j);
                }
            }
        } else {
            // Mark as considered so siblings do not retry the same parent.
            for b in blocks.iter_mut().filter(|b| b.parent_id == Some(pid)) {
                b.parent_text = None;
            }
        }
        i += 1;
    }

    let mut body = String::new();
    let mut sources: Vec<StreamLibrarySource> = Vec::new();
    let mut seen_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (n, b) in blocks.iter().enumerate() {
        let page = b.page.map(|p| format!(" (page {p})")).unwrap_or_default();
        let ts = b
            .timestamp
            .filter(|t| t.is_finite() && *t >= 0.0)
            .map(|t| format!(" [~{t:.1}s]"))
            .unwrap_or_default();
        body.push_str(&format!(
            "--- Source {}: {}{}{} ---\n{}\n\n",
            n + 1,
            b.title,
            page,
            ts,
            b.text
        ));
        if sources.len() < MAX_SOURCES && seen_paths.insert(b.file_path.clone()) {
            sources.push(StreamLibrarySource {
                file_path: b.file_path.clone(),
                display_title: b.title.clone(),
                page_number: b.page.and_then(|p| u32::try_from(p).ok()),
            });
        }
    }

    AssembledLibraryContext {
        body,
        sources,
        blocks: blocks.len(),
    }
}

/// Passages shown per reply when there is no model to write an answer.
const LIBRARY_ONLY_MAX_PASSAGES: usize = 4;
/// Characters of each passage shown in a library-only reply.
const LIBRARY_ONLY_PASSAGE_CHARS: usize = 900;

/// Reply for when no AI model can run: the best-matching library passages, quoted directly.
///
/// The station must stay useful on weak hardware, on low power, or when the model runtime is
/// down. Search needs only the small embedding and reranker models, so the operator still gets
/// the right pages of their own documents. Returns `None` when nothing relevant was found.
pub fn format_library_only_answer(
    hits: &[HybridSearchHit],
    reason: &str,
) -> Option<(String, Vec<StreamLibrarySource>)> {
    let assembled = assemble_library_context(hits, usize::MAX / 2);
    if hits.is_empty() || assembled.sources.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str("**The AI model is not available, so this is a direct search of your library.**\n\n");
    let reason = reason.trim();
    if !reason.is_empty() {
        out.push_str(&format!("_{reason}_\n\n"));
    }
    out.push_str("Closest passages:\n\n");
    let mut shown = 0usize;
    for hit in hits {
        let text = hit.expanded_text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() {
            continue;
        }
        let cut = truncate_at_char_boundary(&text, LIBRARY_ONLY_PASSAGE_CHARS);
        let ellipsis = if cut.len() < text.len() { "…" } else { "" };
        let page = hit
            .page_number
            .map(|p| format!(", page {p}"))
            .unwrap_or_default();
        out.push_str(&format!(
            "**{}. {}{}**\n\n> {}{}\n\n",
            shown + 1,
            title_for(hit),
            page,
            cut,
            ellipsis
        ));
        shown += 1;
        if shown >= LIBRARY_ONLY_MAX_PASSAGES {
            break;
        }
    }
    if shown == 0 {
        return None;
    }
    let mut sources = assembled.sources;
    sources.truncate(LIBRARY_ONLY_MAX_PASSAGES);
    Some((out, sources))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(file: &str, text: &str, page: Option<i64>, parent: Option<(i64, &str)>) -> HybridSearchHit {
        HybridSearchHit {
            expanded_text: text.to_string(),
            display_title: Some(file.trim_end_matches(".pdf").to_string()),
            page_number: page,
            file_path: format!("/lib/{file}"),
            timestamp_start: None,
            score: 1.0,
            parent_id: parent.map(|p| p.0),
            parent_text: parent.map(|p| p.1.to_string()),
        }
    }

    #[test]
    fn strips_quick_mode_prefix() {
        let wrapped = "[Quick mode] Be concise and useful. Lead with the answer, then the key facts. Use lists only when they help. If an image is attached, identify it and answer from what you see.\n\nwho is the yellow knight?";
        assert_eq!(strip_client_prompt_wrapping(wrapped), "who is the yellow knight?");
    }

    #[test]
    fn strips_document_context_wrapper_and_mode_prefix() {
        let wrapped = "[Deep technical mode] Prefer thorough answers.\n\nUse the following document context, spatial map context, and recent radio traffic context to answer the user's question.\n\nDocument Context: some old chunk text\n\nSpatial Map Context: (none)\n\nRecent Radio Traffic Context: Radio Log [t] [rx]: hello\n\nQuestion: how do I purify water?";
        assert_eq!(strip_client_prompt_wrapping(wrapped), "how do I purify water?");
    }

    #[test]
    fn strips_trailing_attachment_notes() {
        let wrapped = "what is this part?\n\n[A local camera or image frame is attached. Identify the person.]";
        assert_eq!(strip_client_prompt_wrapping(wrapped), "what is this part?");
    }

    #[test]
    fn strips_background_block_from_current_clients() {
        let wrapped = "[Quick mode] Be concise.\n\nwhere is the nearest water source?\n\n[Background attached automatically by ATHENA. Use it only if the question is about the map or radio traffic.]\nMap context:\n3 markers nearby";
        assert_eq!(
            strip_client_prompt_wrapping(wrapped),
            "where is the nearest water source?"
        );
    }

    #[test]
    fn leaves_plain_questions_alone() {
        let q = "What does the manual say about [bracketed] things?\n\nSecond paragraph.";
        assert_eq!(strip_client_prompt_wrapping(q), q);
    }

    #[test]
    fn budget_scales_with_context_window_and_history() {
        let small = library_context_char_budget(Some(4096), 400);
        let large = library_context_char_budget(Some(8192), 400);
        let busy = library_context_char_budget(Some(4096), 6000);
        assert!(small >= 6_000 && small <= 9_000, "4k window budget was {small}");
        assert!(large > small);
        assert!(busy < small);
        assert_eq!(library_context_char_budget(Some(1024), 50_000), MIN_LIBRARY_CHARS);
        assert_eq!(library_context_char_budget(None, 400), small);
    }

    #[test]
    fn keeps_hits_in_rank_order_within_budget() {
        let a = "a".repeat(1000);
        let b = "b".repeat(1000);
        let c = "c".repeat(1000);
        let hits = vec![
            hit("one.pdf", &a, Some(3), None),
            hit("two.pdf", &b, None, None),
            hit("three.pdf", &c, None, None),
        ];
        let out = assemble_library_context(&hits, 2_300);
        assert_eq!(out.blocks, 2);
        assert!(out.body.contains("Source 1: one (page 3)"));
        assert!(out.body.contains("Source 2: two"));
        assert!(!out.body.contains("three"));
        assert_eq!(out.sources.len(), 2);
        assert_eq!(out.sources[0].page_number, Some(3));
    }

    #[test]
    fn best_hit_is_truncated_rather_than_dropped() {
        let long = "x".repeat(10_000);
        let out = assemble_library_context(&[hit("big.pdf", &long, None, None)], 2_000);
        assert_eq!(out.blocks, 1);
        assert!(out.body.len() < 2_200);
    }

    #[test]
    fn sources_list_only_documents_in_the_prompt() {
        let hits = vec![
            hit("one.pdf", "alpha", None, None),
            hit("one.pdf", "beta", None, None),
            hit("two.pdf", "gamma", None, None),
        ];
        let out = assemble_library_context(&hits, 10_000);
        let paths: Vec<&str> = out.sources.iter().map(|s| s.file_path.as_str()).collect();
        assert_eq!(paths, vec!["/lib/one.pdf", "/lib/two.pdf"]);
    }

    #[test]
    fn expands_to_parent_section_when_it_fits() {
        let parent = format!("{} CHILD ONE {} CHILD TWO {}", "p".repeat(500), "p".repeat(500), "p".repeat(500));
        let hits = vec![
            hit("one.pdf", "CHILD ONE", None, Some((7, &parent))),
            hit("one.pdf", "CHILD TWO", None, Some((7, &parent))),
            hit("two.pdf", "other", None, None),
        ];
        let out = assemble_library_context(&hits, 10_000);
        assert_eq!(out.blocks, 2, "two children of one parent collapse into the parent");
        assert_eq!(out.body.matches("CHILD ONE").count(), 1);
        assert!(out.body.contains(&"p".repeat(500)));
    }

    #[test]
    fn skips_parent_expansion_when_it_does_not_fit() {
        let parent = "p".repeat(9_000);
        let hits = vec![hit("one.pdf", "CHILD ONE", None, Some((7, &parent)))];
        let out = assemble_library_context(&hits, 2_000);
        assert_eq!(out.blocks, 1);
        assert!(out.body.contains("CHILD ONE"));
        assert!(!out.body.contains("ppppp"));
    }

    #[test]
    fn library_only_answer_quotes_passages_with_titles_and_pages() {
        let hits = vec![
            hit("water.pdf", "Boil water for one minute.", Some(4), None),
            hit("radio.pdf", "Check in at 0700.", None, None),
        ];
        let (text, sources) =
            format_library_only_answer(&hits, "Ollama is not running.").expect("answer");
        assert!(text.contains("AI model is not available"));
        assert!(text.contains("Ollama is not running."));
        assert!(text.contains("**1. water, page 4**"));
        assert!(text.contains("> Boil water for one minute."));
        assert!(text.contains("**2. radio**"));
        assert_eq!(sources.len(), 2);
    }

    #[test]
    fn library_only_answer_is_none_without_hits() {
        assert!(format_library_only_answer(&[], "down").is_none());
    }

    #[test]
    fn empty_hits_produce_empty_context() {
        let out = assemble_library_context(&[], 8_000);
        assert!(out.body.is_empty());
        assert!(out.sources.is_empty());
    }
}
