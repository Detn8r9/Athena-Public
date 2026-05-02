use serde::Serialize;

use super::{ceil_char_boundary, floor_char_boundary};

/// A single chunk of text with optional metadata for embedding and retrieval.
#[derive(Debug, Clone, Serialize)]
pub struct SemanticChunk {
    /// The chunk text. Paragraphs, headers, and tables are kept grouped.
    pub text: String,
    /// Zero-based index of this chunk within the document.
    pub index: usize,
    /// Byte offset of the start of this chunk in the original document (if known).
    pub start_byte: Option<usize>,
    /// Byte offset of the end of this chunk in the original document (if known).
    pub end_byte: Option<usize>,
}

/// Options for semantic chunking. Controls size and merging behavior.
#[derive(Debug, Clone)]
pub struct ChunkOptions {
    /// Maximum characters per chunk. No chunk will exceed this unless a single
    /// block (e.g. one table or paragraph) is larger, in which case it is split
    /// at sentence or line boundaries.
    pub max_chars: usize,
    /// Minimum characters per chunk when merging small blocks. Avoids tiny chunks.
    pub min_chars: usize,
    /// Preferred soft target when merging blocks. Chunker aims for this size
    /// before hitting max_chars.
    pub target_chars: usize,
}

impl Default for ChunkOptions {
    fn default() -> Self {
        Self {
            max_chars: 1200,
            min_chars: 64,
            target_chars: 800,
        }
    }
}

const DEFAULT_MAX_WORDS: usize = 500;
const DEFAULT_OVERLAP_WORDS: usize = 40;
const SENTENCE_LOOKBACK_WORDS: usize = 60;

fn ends_sentence(token: &str) -> bool {
    let t = token.trim_end_matches(['"', '\'', ')', ']', '}']);
    t.ends_with('.') || t.ends_with('!') || t.ends_with('?')
}

fn enforce_word_limit(
    chunks: Vec<SemanticChunk>,
    max_words: usize,
    overlap_words: usize,
) -> Vec<SemanticChunk> {
    let mut out: Vec<SemanticChunk> = Vec::new();
    let mut next_index: usize = 0;

    for c in chunks {
        let words: Vec<&str> = c.text.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }

        if words.len() <= max_words {
            out.push(SemanticChunk {
                text: c.text,
                index: next_index,
                start_byte: c.start_byte,
                end_byte: c.end_byte,
            });
            next_index += 1;
            continue;
        }

        let mut start = 0usize;
        while start < words.len() {
            let mut end = (start + max_words).min(words.len());

            if end < words.len() {
                let lookback = SENTENCE_LOOKBACK_WORDS.min(end.saturating_sub(start));
                let mut chosen: Option<usize> = None;
                for i in (end.saturating_sub(lookback)..end).rev() {
                    if ends_sentence(words[i]) {
                        chosen = Some(i + 1);
                        break;
                    }
                }
                if let Some(e) = chosen {
                    if e > start + 40 {
                        end = e;
                    }
                }
            }

            let Some(window) = words.get(start..end) else {
                eprintln!(
                    "[athena] chunking: enforce_word_limit bad word range {}..{} (len {}); stopping",
                    start,
                    end,
                    words.len()
                );
                break;
            };
            let text = window.join(" ");
            out.push(SemanticChunk {
                text,
                index: next_index,
                start_byte: None,
                end_byte: None,
            });
            next_index += 1;

            if end >= words.len() {
                break;
            }
            let overlap = overlap_words.min(end.saturating_sub(start).saturating_sub(1));
            start = end.saturating_sub(overlap);
        }
    }

    out
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string()
}

fn is_markdown_header(line: &str) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return false;
    }
    let hash_count = trimmed.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hash_count) && trimmed.len() > hash_count {
        let Some(rest) = trimmed.get(hash_count..) else {
            eprintln!("[athena] chunking: is_markdown_header slice failed at byte {}", hash_count);
            return false;
        };
        let rest = rest.trim_start();
        rest.is_empty() || !rest.starts_with('#')
    } else {
        false
    }
}

fn is_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let pipe_count = trimmed.matches('|').count();
    pipe_count >= 2 && trimmed.contains('|')
}

fn is_table_separator(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 || !trimmed.starts_with('|') || !trimmed.ends_with('|') {
        return false;
    }
    let inner_end = floor_char_boundary(trimmed, trimmed.len().saturating_sub(1));
    let Some(inner) = trimmed.get(1..inner_end) else {
        eprintln!("[athena] chunking: is_table_separator inner slice failed");
        return false;
    };
    let inner = inner.trim();
    inner
        .chars()
        .all(|c| c == '-' || c == ':' || c == ' ' || c == '|')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Blank,
    Header,
    TableRow,
    TableSeparator,
    Other,
}

fn line_kind(line: &str) -> LineKind {
    let trimmed = line.trim_end();
    if trimmed.is_empty() {
        return LineKind::Blank;
    }
    if is_markdown_header(trimmed) {
        return LineKind::Header;
    }
    if is_table_separator(trimmed) {
        return LineKind::TableSeparator;
    }
    if is_table_row(trimmed) {
        return LineKind::TableRow;
    }
    LineKind::Other
}

#[derive(Debug)]
struct Block {
    text: String,
    start_byte: usize,
    end_byte: usize,
    is_table: bool,
    is_header_block: bool,
}

fn extract_blocks(text: &str) -> Vec<Block> {
    let normalized = normalize(text);
    if normalized.is_empty() {
        return Vec::new();
    }
    let mut blocks = Vec::new();
    let mut current = String::new();
    let mut current_start = 0usize;
    let mut in_table = false;
    let mut is_header_block = false;
    let mut byte_pos = 0usize;
    #[allow(unused_assignments)]
    let mut line_start_byte = 0usize;
    for line in normalized.split('\n') {
        line_start_byte = byte_pos;
        byte_pos += line.len() + 1;
        let line_kind = line_kind(line);
        match line_kind {
            LineKind::Blank => {
                if !current.is_empty() {
                    let end_byte = line_start_byte;
                    blocks.push(Block {
                        text: current.trim_end().to_string(),
                        start_byte: current_start,
                        end_byte,
                        is_table: in_table,
                        is_header_block,
                    });
                    current = String::new();
                }
                in_table = false;
                is_header_block = false;
            }
            LineKind::Header => {
                if !current.is_empty() && !in_table {
                    let end_byte = line_start_byte;
                    blocks.push(Block {
                        text: current.trim_end().to_string(),
                        start_byte: current_start,
                        end_byte,
                        is_table: false,
                        is_header_block,
                    });
                    current = String::new();
                }
                in_table = false;
                if current.is_empty() {
                    current_start = line_start_byte;
                    is_header_block = true;
                }
                if !current.is_empty() {
                    current.push('\n');
                }
                current.push_str(line);
                current.push('\n');
            }
            LineKind::TableRow | LineKind::TableSeparator => {
                if !in_table && !current.is_empty() {
                    let end_byte = line_start_byte;
                    blocks.push(Block {
                        text: current.trim_end().to_string(),
                        start_byte: current_start,
                        end_byte,
                        is_table: false,
                        is_header_block,
                    });
                    current = String::new();
                    current_start = line_start_byte;
                }
                in_table = true;
                is_header_block = false;
                if current.is_empty() {
                    current_start = line_start_byte;
                }
                if !current.is_empty() {
                    current.push('\n');
                }
                current.push_str(line);
                current.push('\n');
            }
            LineKind::Other => {
                if in_table {
                    if !current.is_empty() {
                        let end_byte = line_start_byte;
                        blocks.push(Block {
                            text: current.trim_end().to_string(),
                            start_byte: current_start,
                            end_byte,
                            is_table: true,
                            is_header_block: false,
                        });
                        current = String::new();
                    }
                    in_table = false;
                }
                if current.is_empty() {
                    current_start = line_start_byte;
                }
                if !current.is_empty() {
                    current.push('\n');
                }
                current.push_str(line);
                current.push('\n');
            }
        }
    }
    if !current.is_empty() {
        blocks.push(Block {
            text: current.trim_end().to_string(),
            start_byte: current_start,
            end_byte: normalized.len(),
            is_table: in_table,
            is_header_block,
        });
    }
    blocks
}

fn split_large_block(block: &Block, max_chars: usize) -> Vec<(String, usize, usize)> {
    if block.text.len() <= max_chars {
        return vec![(block.text.clone(), block.start_byte, block.end_byte)];
    }
    let mut out = Vec::new();
    let mut rest = block.text.as_str();
    let mut offset = block.start_byte;
    let mut guard: usize = 0;
    let guard_max = rest.len().saturating_add(8);

    while rest.len() > max_chars {
        guard += 1;
        if guard > guard_max {
            eprintln!("[athena] chunking: split_large_block guard tripped; keeping remainder as one piece");
            break;
        }

        let mut safe_max = floor_char_boundary(rest, max_chars);
        if safe_max == 0 && !rest.is_empty() {
            safe_max = ceil_char_boundary(rest, 1);
        }
        if safe_max == 0 {
            eprintln!("[athena] chunking: split_large_block cannot compute safe prefix; stopping");
            break;
        }

        let search = match rest.get(..safe_max) {
            Some(s) => s,
            None => {
                eprintln!(
                    "[athena] chunking: split_large_block prefix get(..{}) failed",
                    safe_max
                );
                break;
            }
        };

        let mut split_at = search
            .rfind("\n\n")
            .or_else(|| search.rfind(". "))
            .or_else(|| search.rfind(".\n"))
            .or_else(|| search.rfind('\n'))
            .map(|i| i.saturating_add(1))
            .unwrap_or(safe_max);
        split_at = split_at.min(safe_max);
        split_at = floor_char_boundary(rest, split_at);
        if split_at == 0 && !rest.is_empty() {
            split_at = ceil_char_boundary(rest, 1);
        }
        if split_at == 0 || split_at > rest.len() {
            eprintln!(
                "[athena] chunking: split_large_block invalid split_at {} (rest len {})",
                split_at,
                rest.len()
            );
            break;
        }

        let piece = match rest.get(..split_at) {
            Some(s) => s.trim_end(),
            None => {
                eprintln!(
                    "[athena] chunking: split_large_block piece get(..{}) failed",
                    split_at
                );
                break;
            }
        };

        if piece.is_empty() {
            eprintln!("[athena] chunking: split_large_block empty piece; advancing one char");
            let adv = ceil_char_boundary(rest, 1);
            if adv == 0 || adv > rest.len() {
                break;
            }
            if let Some(chunk) = rest.get(..adv) {
                if !chunk.is_empty() {
                    let end_byte = offset + chunk.len();
                    out.push((chunk.to_string(), offset, end_byte));
                }
            }
            offset += adv;
            rest = rest.get(adv..).map(|s| s.trim_start()).unwrap_or("");
            continue;
        }

        let end_byte = offset + piece.len();
        out.push((piece.to_string(), offset, end_byte));
        offset += split_at;
        rest = match rest.get(split_at..) {
            Some(t) => t.trim_start(),
            None => {
                eprintln!(
                    "[athena] chunking: split_large_block tail get({}..) failed",
                    split_at
                );
                ""
            }
        };
        if rest.is_empty() {
            break;
        }
    }
    if !rest.is_empty() {
        out.push((rest.to_string(), offset, block.end_byte));
    }
    out
}

pub fn chunk_text(text: &str, options: &ChunkOptions) -> Vec<SemanticChunk> {
    let blocks = extract_blocks(text);
    if blocks.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut chunk_index = 0usize;
    let mut buffer = String::new();
    let mut buffer_start: Option<usize> = None;
    let mut buffer_end: Option<usize> = None;
    let max_chars = options.max_chars;
    let min_chars = options.min_chars;
    let target_chars = options.target_chars;

    fn flush(
        buffer: &mut String,
        buffer_start: &mut Option<usize>,
        buffer_end: &mut Option<usize>,
        out: &mut Vec<SemanticChunk>,
        chunk_index: &mut usize,
    ) {
        let text = buffer.trim_end();
        if text.is_empty() {
            return;
        }
        out.push(SemanticChunk {
            text: text.to_string(),
            index: *chunk_index,
            start_byte: *buffer_start,
            end_byte: *buffer_end,
        });
        *chunk_index += 1;
        buffer.clear();
        *buffer_start = None;
        *buffer_end = None;
    }

    for block in blocks {
        if block.text.len() > max_chars {
            flush(
                &mut buffer,
                &mut buffer_start,
                &mut buffer_end,
                &mut out,
                &mut chunk_index,
            );
            for (piece, start, end) in split_large_block(&block, max_chars) {
                out.push(SemanticChunk {
                    text: piece,
                    index: chunk_index,
                    start_byte: Some(start),
                    end_byte: Some(end),
                });
                chunk_index += 1;
            }
            continue;
        }
        let would_exceed = buffer.len() + 1 + block.text.len() > max_chars;
        let at_target = buffer.len() >= target_chars;
        let header_or_table = block.is_header_block || block.is_table;
        let should_flush = would_exceed
            || (at_target && !header_or_table && buffer.len() + 1 + block.text.len() <= max_chars)
            || (header_or_table && !buffer.is_empty() && (block.is_header_block || block.is_table));
        if should_flush && !buffer.is_empty() {
            flush(
                &mut buffer,
                &mut buffer_start,
                &mut buffer_end,
                &mut out,
                &mut chunk_index,
            );
        }
        if buffer.is_empty() {
            buffer_start = Some(block.start_byte);
        }
        buffer_end = Some(block.end_byte);
        if !buffer.is_empty() {
            buffer.push('\n');
            buffer.push('\n');
        }
        buffer.push_str(&block.text);
    }
    if buffer.len() >= min_chars || (!buffer.is_empty() && out.is_empty()) {
        flush(
            &mut buffer,
            &mut buffer_start,
            &mut buffer_end,
            &mut out,
            &mut chunk_index,
        );
    }
    out
}

pub fn chunk_text_default(text: &str) -> Vec<SemanticChunk> {
    let chunks = chunk_text(text, &ChunkOptions::default());
    enforce_word_limit(chunks, DEFAULT_MAX_WORDS, DEFAULT_OVERLAP_WORDS)
}
