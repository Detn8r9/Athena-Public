//! Chat search over the offline archives (ZIM files: Wikipedia, WikiMed, iFixit, …).
//!
//! A ZIM carries its own full-text index, and kiwix-serve searches every archive it serves in one
//! request. Copying a whole encyclopedia into the chat index would take days and many times the
//! disk space, so chat asks kiwix-serve instead: the best-matching articles are fetched, cut into
//! passages, and the passages that mention the question's words go to the same reranker as the
//! user's own documents. Every archive is searchable as soon as it is served.
//!
//! This runs before the database lock is taken, so a slow disk never holds up the rest of the app.

use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct KiwixPassage {
    /// Archive title, e.g. "WikiMed Medical Encyclopedia".
    pub book: String,
    /// Article title.
    pub title: String,
    /// Path on kiwix-serve, e.g. `/kiwix/content/wikipedia_en_all/Water_purification`.
    pub link: String,
    pub text: String,
}

impl KiwixPassage {
    /// Source key used in citations; the chat window opens it in the library reader.
    pub fn source_path(&self) -> String {
        format!("{SOURCE_SCHEME}{}", self.link)
    }

    pub fn display_title(&self) -> String {
        if self.book.is_empty() {
            self.title.clone()
        } else {
            format!("{} — {}", self.title, self.book)
        }
    }
}

/// Prefix of citation paths that point into the offline library rather than a file.
pub const SOURCE_SCHEME: &str = "kiwix:";

const SEARCH_TIMEOUT: Duration = Duration::from_secs(8);
const ARTICLE_TIMEOUT: Duration = Duration::from_secs(5);
/// Search results considered (several archives often carry the same article).
const RESULTS: usize = 10;
/// Articles read in full.
const ARTICLES: usize = 5;
const PASSAGES_PER_ARTICLE: usize = 3;
const PASSAGE_WORDS: usize = 170;
const PASSAGE_STEP: usize = 120;
/// Article text read, at most. Long articles keep their lead and most relevant parts.
const ARTICLE_MAX_CHARS: usize = 200_000;

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "are", "but", "not", "you", "all", "any", "can", "had", "her", "was", "one", "our", "out",
    "has", "him", "his", "how", "its", "may", "new", "now", "old", "see", "two", "who", "did", "get", "let", "say",
    "she", "too", "use", "what", "when", "where", "which", "with", "would", "could", "should", "there", "their",
    "this", "that", "these", "those", "from", "have", "into", "about", "your", "they", "them", "then", "than",
    "will", "just", "some", "does", "doing", "best", "way", "ways", "tell", "know", "need", "want", "like", "make",
    "please", "explain", "give", "show", "much", "many", "also", "been", "being", "were", "why", "more", "most",
];

/// Content words of a question, lowercase, for picking passages.
pub fn query_terms(query: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in query.split(|c: char| !c.is_alphanumeric()) {
        let w = w.to_lowercase();
        if w.chars().count() < 3 || STOPWORDS.contains(&w.as_str()) || out.contains(&w) {
            continue;
        }
        out.push(w);
    }
    out
}

/// Passages from the offline archives that bear on `query`. Empty when kiwix-serve is not
/// running, nothing matches, or the search is too slow.
pub fn search_passages(query: &str) -> Vec<KiwixPassage> {
    let terms = query_terms(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let pattern: String = query.chars().take(200).collect();
    let base = crate::kiwix_serve::local_base_url();
    let enc = urlencoding::encode(pattern.trim());
    let mut hits = fetch_hits(&format!(
        "{base}/search?pattern={enc}&format=xml&pageLength={RESULTS}&books.filter.lang=eng"
    ));
    if hits.is_empty() {
        // Archives without a language tag are left out by the filter.
        hits = fetch_hits(&format!("{base}/search?pattern={enc}&format=xml&pageLength={RESULTS}"));
    }
    hits.truncate(ARTICLES);
    if hits.is_empty() {
        return Vec::new();
    }

    let origin = format!("http://127.0.0.1:{}", crate::kiwix_serve::KIWIX_SERVE_PORT);
    let pages: Vec<Option<String>> = std::thread::scope(|s| {
        let handles: Vec<_> = hits
            .iter()
            .map(|h| {
                let url = format!("{origin}{}", h.link);
                s.spawn(move || {
                    ureq::get(&url)
                        .timeout(ARTICLE_TIMEOUT)
                        .call()
                        .ok()
                        .and_then(|r| r.into_string().ok())
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().ok().flatten()).collect()
    });

    let mut out = Vec::new();
    for (hit, page) in hits.iter().zip(pages) {
        let text = match page {
            Some(html) => html_to_text(&html),
            None => String::new(),
        };
        // Fall back to the search snippet when the article could not be read.
        let text = if text.split_whitespace().count() < 20 { hit.snippet.clone() } else { text };
        for passage in best_passages(&text, &terms, PASSAGES_PER_ARTICLE) {
            out.push(KiwixPassage {
                book: hit.book.clone(),
                title: hit.title.clone(),
                link: hit.link.clone(),
                text: passage,
            });
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
struct Hit {
    title: String,
    link: String,
    book: String,
    snippet: String,
}

fn fetch_hits(url: &str) -> Vec<Hit> {
    match ureq::get(url).timeout(SEARCH_TIMEOUT).call() {
        Ok(r) => r.into_string().map(|x| parse_search_xml(&x)).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn tag_text<'a>(block: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = block.find(open)? + open.len();
    let end = block[start..].find(close)? + start;
    Some(&block[start..end])
}

/// Results of kiwix-serve's OpenSearch (`format=xml`) answer, one per article title.
fn parse_search_xml(xml: &str) -> Vec<Hit> {
    let mut out: Vec<Hit> = Vec::new();
    for chunk in xml.split("<item>").skip(1) {
        let item = chunk.split("</item>").next().unwrap_or(chunk);
        let (Some(title), Some(link)) = (tag_text(item, "<title>", "</title>"), tag_text(item, "<link>", "</link>")) else {
            continue;
        };
        let book = item
            .split("<book>")
            .nth(1)
            .and_then(|b| tag_text(b, "<title>", "</title>"))
            .unwrap_or("");
        let snippet = tag_text(item, "<description>", "</description>").unwrap_or("");
        let title = decode_entities(title).trim().to_string();
        let link = decode_entities(link).trim().to_string();
        if title.is_empty() || !link.starts_with('/') {
            continue;
        }
        // The same article often sits in several archives (WikiMed, MDWiki, Wikipedia).
        let key = title.to_lowercase();
        if out.iter().any(|h| h.title.to_lowercase() == key) {
            continue;
        }
        out.push(Hit {
            title,
            link,
            book: decode_entities(book).trim().to_string(),
            snippet: strip_tags(&decode_entities(snippet)).replace("...", " ").trim().to_string(),
        });
    }
    out
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(semi) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..semi];
        let decoded = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" | "#160" => Some(' '),
            _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32)
            }
            _ if ent.starts_with('#') => ent[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Readable text of an article page: no scripts, styles, navigation or markup.
pub fn html_to_text(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    // Skip <head>, and drop script/style/nav blocks wholesale.
    let body_start = lower.find("<body").unwrap_or(0);
    let mut out = String::with_capacity(html.len().min(ARTICLE_MAX_CHARS) / 2);
    let mut i = body_start;
    let bytes = html.as_bytes();
    while i < html.len() && out.len() < ARTICLE_MAX_CHARS {
        if bytes[i] == b'<' {
            let rest = &lower[i..];
            let mut skipped = false;
            for tag in ["script", "style", "nav", "noscript", "svg", "math"] {
                if rest.starts_with(&format!("<{tag}")) {
                    let close = format!("</{tag}>");
                    i = match rest.find(&close) {
                        Some(e) => i + e + close.len(),
                        None => html.len(),
                    };
                    skipped = true;
                    break;
                }
            }
            if skipped {
                continue;
            }
            let end = match html[i..].find('>') {
                Some(e) => i + e + 1,
                None => html.len(),
            };
            let tag = &lower[i..end];
            if ["<p", "</p", "<br", "<li", "</li", "<h1", "<h2", "<h3", "<h4", "</h", "<div", "</div", "<tr", "</tr", "<dt", "<dd"]
                .iter()
                .any(|t| tag.starts_with(t))
            {
                out.push('\n');
            } else {
                out.push(' ');
            }
            i = end;
            continue;
        }
        let next = html[i..].find('<').map(|n| i + n).unwrap_or(html.len());
        out.push_str(&decode_entities(&html[i..next]));
        i = next;
    }
    out.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The passages of `text` that mention the most query terms, in article order. An article with
/// no matching passage gives its lead (search found it for a reason).
pub fn best_passages(text: &str, terms: &[String], n: usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() || n == 0 {
        return Vec::new();
    }
    let mut windows: Vec<(f32, usize)> = Vec::new();
    let mut start = 0;
    loop {
        let end = (start + PASSAGE_WORDS).min(words.len());
        let lowered: Vec<String> = words[start..end]
            .iter()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
            .collect();
        let distinct = terms.iter().filter(|t| lowered.iter().any(|w| w.starts_with(t.as_str()))).count();
        let hits = lowered.iter().filter(|w| terms.iter().any(|t| w.starts_with(t.as_str()))).count();
        if distinct > 0 {
            windows.push((distinct as f32 + (hits as f32).min(12.0) * 0.1, start));
        }
        if end == words.len() {
            break;
        }
        start += PASSAGE_STEP;
    }
    let mut picked: Vec<usize> = Vec::new();
    windows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
    for (_, s) in windows {
        if picked.iter().all(|p| p.abs_diff(s) >= PASSAGE_WORDS) {
            picked.push(s);
        }
        if picked.len() == n {
            break;
        }
    }
    if picked.is_empty() {
        picked.push(0);
    }
    picked.sort_unstable();
    picked
        .into_iter()
        .map(|s| words[s..(s + PASSAGE_WORDS).min(words.len())].join(" "))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?><rss><channel>
    <item>
      <title>Portable water purification</title>
      <link>/kiwix/content/mdwiki_en_all/Portable_water_purification</link>
        <description>...<b>water</b> purification devices are self-contained...</description>
        <book><title>MDWiki Medical Encyclopedia</title></book>
    </item>
    <item>
      <title>Portable water purification</title>
      <link>/kiwix/content/wikipedia_en_medicine/Portable_water_purification</link>
        <book><title>WikiMed</title></book>
    </item>
    <item>
      <title>Tom &amp; Jerry&#39;s guide</title>
      <link>/kiwix/content/x/Tom</link>
        <book><title>X</title></book>
    </item>
    </channel></rss>"#;

    #[test]
    fn search_results_are_read_and_duplicate_articles_dropped() {
        let hits = parse_search_xml(SAMPLE);
        assert_eq!(hits.len(), 2, "the same article from a second archive is skipped");
        assert_eq!(hits[0].book, "MDWiki Medical Encyclopedia");
        assert_eq!(hits[0].link, "/kiwix/content/mdwiki_en_all/Portable_water_purification");
        assert!(hits[0].snippet.contains("water purification devices"));
        assert_eq!(hits[1].title, "Tom & Jerry's guide");
    }

    #[test]
    fn article_pages_become_plain_text() {
        let html = "<html><head><title>T</title><style>p{}</style></head><body><nav>Menu</nav>\
            <h1>Snake bite</h1><p>Keep the person&nbsp;calm.</p><script>x()</script><ul><li>Remove rings</li></ul></body></html>";
        let t = html_to_text(html);
        assert_eq!(t, "Snake bite\nKeep the person calm.\nRemove rings");
    }

    #[test]
    fn passages_follow_the_question() {
        let mut text = String::new();
        for i in 0..400 {
            text.push_str(&format!("filler{i} "));
        }
        text.push_str("To purify water boil it for one minute. ");
        for i in 0..400 {
            text.push_str(&format!("more{i} "));
        }
        let terms = query_terms("How do I purify water?");
        assert_eq!(terms, ["purify", "water"]);
        let p = best_passages(&text, &terms, 3);
        assert!(!p.is_empty());
        assert!(p[0].contains("purify water"), "{}", p[0]);
        // No match: the lead is used.
        let lead = best_passages("alpha beta gamma", &terms, 3);
        assert_eq!(lead, ["alpha beta gamma"]);
    }

    #[test]
    fn citations_point_into_the_library_reader() {
        let p = KiwixPassage {
            book: "WikiMed".into(),
            title: "Burn".into(),
            link: "/kiwix/content/wikimed/Burn".into(),
            text: String::new(),
        };
        assert_eq!(p.source_path(), "kiwix:/kiwix/content/wikimed/Burn");
        assert_eq!(p.display_title(), "Burn — WikiMed");
    }
}
