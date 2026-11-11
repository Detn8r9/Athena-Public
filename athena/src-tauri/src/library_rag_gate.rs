//! Fast heuristic gate: skip library / hybrid RAG when the user is not asking
//! for document knowledge. Avoids embedding + retrieval latency (and junk cites)
//! on greetings, confirmations, and Athena-capability questions.

use regex::Regex;
use std::sync::OnceLock;

/// Prefer-faster surfaces (Conversation / voice) skip more aggressively on short ambiguous turns.
pub fn should_skip_library_rag(query: &str, prefer_fast: bool) -> bool {
    let q = normalize_query(query);
    if q.is_empty() {
        return true;
    }
    // Explicit research / document intent always wins.
    if needs_library(&q) {
        return false;
    }
    // Clear social / control turns never need the archive.
    if is_chitchat_or_control(&q) {
        return true;
    }
    let words = word_count(&q);
    // Short ambiguous turns: Conversation skips; typed Chat still retrieves
    // unless already caught as chitchat above.
    if prefer_fast && words <= 8 && !has_factual_signal(&q) {
        return true;
    }
    if words <= 2 && !q.contains('?') {
        return true;
    }
    false
}

fn normalize_query(raw: &str) -> String {
    let mut t = raw.trim().to_lowercase();
    for prefix in [
        "[live voice call]",
        "[deep technical mode]",
        "[visual input is attached",
    ] {
        if t.starts_with(prefix) {
            if let Some((_, rest)) = t.split_once('\n') {
                t = rest.trim().to_string();
            }
        }
    }
    t = t
        .trim_matches(|c: char| matches!(c, '!' | '?' | '.' | '…' | ',' | ';' | ':'))
        .trim()
        .to_string();
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn word_count(q: &str) -> usize {
    q.split_whitespace().filter(|w| !w.is_empty()).count()
}

fn needs_library(q: &str) -> bool {
    if re_needs().is_match(q) {
        return true;
    }
    // Informational questions that usually need knowledge beyond chitchat
    if q.contains('?') && re_question().is_match(q) && has_factual_signal(q) {
        return true;
    }
    false
}

fn is_chitchat_or_control(q: &str) -> bool {
    if re_chitchat().is_match(q) {
        return true;
    }
    // Very short acknowledgements
    if word_count(q) <= 4 && re_ack().is_match(q) {
        return true;
    }
    false
}

fn has_factual_signal(q: &str) -> bool {
    re_factual().is_match(q)
}

fn re_needs() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(look[\s-]?up|lookup|search|find|retrieve|cite|quote|according to|in the (library|docs?|manual|handbook|sop|pdf|book|file|document|report|guide))\b|\b(library|document|manual|handbook|sop|pdf|procedure|checklist|field guide|ops manual|doctrine)\b|\b(what does .+ say|where (in|does) .+ (say|mention)|summarize (the|this|that) )\b|\b(page \d+|section \d+|chapter \d+)\b|\.(pdf|docx?|txt|md|html)\b",
        )
        .expect("needs_library regex")
    })
}

fn re_question() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(how (do|does|to|can|should)|what (is|are|was|were|does|do)|where (is|are|was|can)|when (is|are|was|did)|why (is|are|do|does)|who (is|are|was))\b",
        )
        .expect("question regex")
    })
}

fn re_chitchat() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^(hi|hello|hey|yo|sup|howdy|hiya|good (morning|afternoon|evening)|morning|evening|thanks|thank you|ty|bye|goodbye|see you|ok|okay|k|yes|yeah|yep|yup|no|nope|nah|sure|cool|great|nice|wow|huh|hm+|umm+|ah+|oh+|how are you( doing)?|how('?s| is) it going|what('?s| is) up)$|^(hi|hello|hey|yo)\b.{0,28}$|\b(are you (there|online|awake|listening|ok|okay)|can you hear me|you there)\b|\b(who are you|what('?s| is) your name|what can you do|how do you work)\b|\b(repeat that|say that again|louder|quieter|stop talking|never ?mind|cancel)\b|\b(tell me a joke|sing (a |me )?song|good night|goodnight)\b",
        )
        .expect("chitchat regex")
    })
}

fn re_ack() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(thanks|thank you|got it|sounds good|will do|roger|copy that|understood)\b",
        )
        .expect("ack regex")
    })
}

fn re_factual() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(map|radio|mesh|inventory|supply|medical|triage|route|gps|coordinate|weather|storm|battery|power|generator|water|food|shelter|comms|frequency|protocol|security|clearance|personnel|logistics|warehouse|vehicle|drone|sensor|temperature|pressure|altitude|lat|lon|utm|athena|project|sop|ops|mission|objective|threat|hazard|chemical|radiation|infection|quarantine)\b|\d{2,}",
        )
        .expect("factual regex")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_hello() {
        assert!(should_skip_library_rag("hello", true));
        assert!(should_skip_library_rag("Hi Athena!", false));
        assert!(should_skip_library_rag("thanks", true));
        assert!(should_skip_library_rag("how are you", true));
        assert!(should_skip_library_rag("how are you?", false));
    }

    #[test]
    fn uses_document_queries() {
        assert!(!should_skip_library_rag("look up the med kit SOP", true));
        assert!(!should_skip_library_rag(
            "what does the field manual say about water purification?",
            false
        ));
    }

    #[test]
    fn conversation_skips_short_ambiguous() {
        assert!(should_skip_library_rag("tell me more", true));
    }

    #[test]
    fn keeps_domain_questions() {
        assert!(!should_skip_library_rag(
            "how do I purify water in the field?",
            true
        ));
    }
}
