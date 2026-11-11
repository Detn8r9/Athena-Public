/**
 * Shared heuristic: skip library / hybrid RAG when the turn does not need docs.
 * Mirrors `library_rag_gate.rs` so Chat + Conversation can log the decision;
 * Rust remains the source of truth on send.
 */

export type LibraryRagGateOptions = {
  /** Conversation / voice: skip short ambiguous turns more aggressively. */
  preferFast?: boolean;
};

export function shouldSkipLibraryRag(
  text: string,
  options: LibraryRagGateOptions = {},
): boolean {
  const preferFast = options.preferFast === true;
  const q = normalizeQuery(text);
  if (!q) return true;
  if (needsLibrary(q)) return false;
  if (isChitchatOrControl(q)) return true;
  const words = wordCount(q);
  if (preferFast && words <= 8 && !hasFactualSignal(q)) return true;
  if (words <= 2 && !q.includes("?")) return true;
  return false;
}

/** @deprecated Prefer shouldSkipLibraryRag(text, { preferFast: true }) */
export function isConversationalChitchat(text: string): boolean {
  return shouldSkipLibraryRag(text, { preferFast: true });
}

function normalizeQuery(raw: string): string {
  let t = raw.trim().toLowerCase();
  for (const prefix of [
    "[live voice call]",
    "[deep technical mode]",
    "[visual input is attached",
  ]) {
    if (t.startsWith(prefix)) {
      const nl = t.indexOf("\n");
      if (nl >= 0) t = t.slice(nl + 1).trim();
    }
  }
  t = t.replace(/^[!?.…,;:]+|[!?.…,;:]+$/g, "").trim();
  return t.replace(/\s+/g, " ");
}

function wordCount(q: string): number {
  return q.split(/\s+/).filter(Boolean).length;
}

function needsLibrary(q: string): boolean {
  if (
    /\b(look[\s-]?up|lookup|search|find|retrieve|cite|quote|according to|in the (library|docs?|manual|handbook|sop|pdf|book|file|document|report|guide))\b/i.test(
      q,
    ) ||
    /\b(library|document|manual|handbook|sop|pdf|procedure|checklist|field guide|ops manual|doctrine)\b/i.test(
      q,
    ) ||
    /\b(what does .+ say|where (in|does) .+ (say|mention)|summarize (the|this|that) )/i.test(
      q,
    ) ||
    /\b(page \d+|section \d+|chapter \d+)\b/i.test(q) ||
    /\.(pdf|docx?|txt|md|html)\b/i.test(q)
  ) {
    return true;
  }
  if (
    q.includes("?") &&
    /\b(how (do|does|to|can|should)|what (is|are|was|were|does|do)|where (is|are|was|can)|when (is|are|was|did)|why (is|are|do|does)|who (is|are|was))\b/i.test(
      q,
    ) &&
    hasFactualSignal(q)
  ) {
    return true;
  }
  return false;
}

function isChitchatOrControl(q: string): boolean {
  if (
    /^(hi|hello|hey|yo|sup|howdy|hiya|good (morning|afternoon|evening)|morning|evening|thanks|thank you|ty|bye|goodbye|see you|ok|okay|k|yes|yeah|yep|yup|no|nope|nah|sure|cool|great|nice|wow|huh|hm+|umm+|ah+|oh+|how are you( doing)?|how('?s| is) it going|what('?s| is) up)$/i.test(
      q,
    ) ||
    /^(hi|hello|hey|yo)\b.{0,28}$/i.test(q) ||
    /\b(are you (there|online|awake|listening|ok|okay)|can you hear me|you there)\b/i.test(
      q,
    ) ||
    /\b(who are you|what('?s| is) your name|what can you do|how do you work)\b/i.test(
      q,
    ) ||
    /\b(repeat that|say that again|louder|quieter|stop talking|never ?mind|cancel)\b/i.test(
      q,
    ) ||
    /\b(tell me a joke|sing (a |me )?song|good night|goodnight)\b/i.test(q)
  ) {
    return true;
  }
  if (
    wordCount(q) <= 4 &&
    /\b(thanks|thank you|got it|sounds good|will do|roger|copy that|understood)\b/i.test(
      q,
    )
  ) {
    return true;
  }
  return false;
}

function hasFactualSignal(q: string): boolean {
  return /\b(map|radio|mesh|inventory|supply|medical|triage|route|gps|coordinate|weather|storm|battery|power|generator|water|food|shelter|comms|frequency|protocol|security|clearance|personnel|logistics|warehouse|vehicle|drone|sensor|temperature|pressure|altitude|lat|lon|utm|athena|project|sop|ops|mission|objective|threat|hazard|chemical|radiation|infection|quarantine)\b/i.test(
    q,
  ) || /\d{2,}/.test(q);
}
