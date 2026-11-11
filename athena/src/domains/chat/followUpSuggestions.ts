/** Follow-up chips after the first assistant answer. */
export function buildFollowUpSuggestions(userText: string, assistantText: string): string[] {
  const u = userText.toLowerCase();
  const a = assistantText.toLowerCase();
  const out: string[] = [];

  if (/water|purify|filter|boil/.test(u) || /water|purify/.test(a)) {
    out.push("What gear do I need for that?");
    out.push("Any risks I should watch for?");
  } else if (/pack|hike|trip|kit|gear/.test(u)) {
    out.push("Make a weight-optimized list");
    out.push("What can I leave behind?");
  } else if (/first aid|injury|wound|frostbite|burn/.test(u) || /first aid|treat/.test(a)) {
    out.push("Walk me through step by step");
    out.push("When should I seek help?");
  } else if (/map|route|navigate|gps/.test(u)) {
    out.push("Give me landmarks to watch for");
    out.push("What’s a safer alternate?");
  } else {
    out.push("Explain that more simply");
    out.push("Give me the next actionable step");
  }

  if (assistantText.length > 400) {
    out.push("Summarize in 3 bullets");
  } else {
    out.push("Go deeper on the technical details");
  }

  // Dedupe, max 3
  return [...new Set(out)].slice(0, 3);
}
