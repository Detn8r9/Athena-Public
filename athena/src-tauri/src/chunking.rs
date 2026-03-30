//! Semantic chunking: split document text into logical units (paragraphs, sections,
//! tables) so the AI keeps formatting context. No hard character splits inside
//! paragraphs or table rows.

#[allow(dead_code)]
mod legacy_semantic;

// --- Parent/child RAG chunking (Phase 1): ~1000-word parents, ~250-word children ---

const PARENT_WORDS: usize = 1000;
const PARENT_STRIDE: usize = 950;
const CHILD_WORDS: usize = 250;
const CHILD_STRIDE: usize = 230;

/// Largest byte index `i` in `0..=byte_idx` on a UTF-8 char boundary (for safe `str` slicing).
pub(super) fn floor_char_boundary(s: &str, byte_idx: usize) -> usize {
    let mut i = byte_idx.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Smallest byte index `i` in `byte_idx..=len` on a UTF-8 char boundary.
pub(super) fn ceil_char_boundary(s: &str, byte_idx: usize) -> usize {
    let mut i = byte_idx.min(s.len());
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// One whitespace-delimited token with optional 1-based PDF page number.
#[derive(Debug, Clone)]
pub struct WordPage {
    pub page: Option<u32>,
    pub word: String,
}

pub fn words_from_plain_text(body: &str) -> Vec<WordPage> {
    body.split_whitespace()
        .filter(|w| !w.is_empty())
        .map(|w| WordPage {
            page: None,
            word: w.to_string(),
        })
        .collect()
}

pub fn words_from_page_map(pages: &[(u32, String)]) -> Vec<WordPage> {
    let mut v = Vec::new();
    for (pn, text) in pages {
        for w in text.split_whitespace() {
            if w.is_empty() {
                continue;
            }
            v.push(WordPage {
                page: Some(*pn),
                word: w.to_string(),
            });
        }
    }
    v
}

/// Prefix for embedding/FTS (Phase 2): document title and page location before body text.
pub fn build_retrieval_text(
    document_title: &str,
    page_range: Option<(u32, u32)>,
    body: &str,
) -> String {
    let title = document_title.trim();
    let body = body.trim();
    if title.is_empty() {
        return match page_range {
            Some((a, b)) if a == b => format!("Page {}.\n\n{}", a, body),
            Some((a, b)) => format!("Pages {}-{}.\n\n{}", a, b, body),
            None => body.to_string(),
        };
    }
    match page_range {
        Some((a, b)) if a == b => {
            format!("Document: {}.\nLocation: page {}.\n\n{}", title, a, body)
        }
        Some((a, b)) => format!(
            "Document: {}.\nLocation: pages {}-{}.\n\n{}",
            title, a, b, body
        ),
        None => format!("Document: {}.\n\n{}", title, body),
    }
}

#[derive(Debug)]
pub struct ChildBlock {
    pub chunk_text: String,
    pub retrieval_text: String,
    pub page_start: Option<i32>,
    pub page_end: Option<i32>,
}

#[derive(Debug)]
pub struct ParentBlock {
    pub chunk_text: String,
    pub retrieval_text: String,
    pub page_start: Option<i32>,
    pub page_end: Option<i32>,
    pub children: Vec<ChildBlock>,
}

fn join_words(words: &[WordPage]) -> String {
    words
        .iter()
        .map(|w| w.word.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn page_range_for_slice(words: &[WordPage]) -> Option<(u32, u32)> {
    let mut pages: Vec<u32> = words.iter().filter_map(|w| w.page).collect();
    if pages.is_empty() {
        return None;
    }
    pages.sort_unstable();
    let a = *pages.first()?;
    let b = *pages.last()?;
    Some((a, b))
}

/// Build parent windows over the full word stream, then child windows inside each parent.
pub fn build_parent_child_plan(words: &[WordPage], document_title: &str) -> Vec<ParentBlock> {
    if words.is_empty() {
        return Vec::new();
    }
    let mut parents = Vec::new();
    let mut p_start = 0usize;
    while p_start < words.len() {
        let p_end = (p_start + PARENT_WORDS).min(words.len());
        let Some(p_slice) = words.get(p_start..p_end) else {
            eprintln!(
                "[athena] chunking: invalid parent word range {}..{} (len {}); stopping",
                p_start,
                p_end,
                words.len()
            );
            break;
        };
        let parent_text = join_words(p_slice);
        let pr = page_range_for_slice(p_slice);
        let parent_retrieval = build_retrieval_text(document_title, pr, &parent_text);

        let mut children = Vec::new();
        let mut c_start = 0usize;
        loop {
            let c_end = (c_start + CHILD_WORDS).min(p_slice.len());
            let Some(c_slice) = p_slice.get(c_start..c_end) else {
                eprintln!(
                    "[athena] chunking: invalid child word range {}..{} in parent; skipping remainder",
                    c_start, c_end
                );
                break;
            };
            let ct = join_words(c_slice);
            let cr = page_range_for_slice(c_slice);
            let crt = build_retrieval_text(document_title, cr, &ct);
            children.push(ChildBlock {
                chunk_text: ct,
                retrieval_text: crt,
                page_start: cr.map(|(a, _)| a as i32),
                page_end: cr.map(|(_, b)| b as i32),
            });
            if c_end >= p_slice.len() {
                break;
            }
            c_start += CHILD_STRIDE;
        }

        parents.push(ParentBlock {
            chunk_text: parent_text,
            retrieval_text: parent_retrieval,
            page_start: pr.map(|(a, _)| a as i32),
            page_end: pr.map(|(_, b)| b as i32),
            children,
        });

        if p_end >= words.len() {
            break;
        }
        p_start += PARENT_STRIDE;
    }
    parents
}
