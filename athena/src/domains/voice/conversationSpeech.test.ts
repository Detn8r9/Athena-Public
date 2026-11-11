import { describe, expect, it } from "vitest";
import {
  SpokenSentenceBuffer,
  adaptiveSilenceOffMs,
  cleanTextForSpeech,
  extractFirstSpeakableSentence,
  isJunkTranscript,
  pickConversationalSpeakText,
} from "./conversationSpeech";

describe("conversationSpeech", () => {
  it("strips markdown for TTS", () => {
    expect(cleanTextForSpeech("**Hello** world")).toBe("Hello world");
    expect(cleanTextForSpeech("See [docs](https://x.test)")).toBe("See docs");
  });

  it("picks a short spoken turn", () => {
    const text =
      "Pack water and a headlamp. Then check the radio. After that print the roster and brief everyone at dawn.";
    const spoken = pickConversationalSpeakText(text, 280);
    expect(spoken.startsWith("Pack water")).toBe(true);
    expect(spoken.length).toBeLessThan(text.length);
  });

  it("extracts first speakable sentence from a stream", () => {
    expect(extractFirstSpeakableSentence("Hello there")).toBeNull();
    expect(extractFirstSpeakableSentence("Hello there. More coming")).toBe("Hello there.");
  });

  it("filters whisper junk but keeps real short answers", () => {
    expect(isJunkTranscript("Thanks for watching.")).toBe(true);
    expect(isJunkTranscript("Thank you.")).toBe(true);
    expect(isJunkTranscript("Thanks")).toBe(true);
    expect(isJunkTranscript("...")).toBe(true);
    expect(isJunkTranscript("yes")).toBe(false);
    expect(isJunkTranscript("okay")).toBe(false);
    expect(isJunkTranscript("Thank you for the map")).toBe(false);
    expect(isJunkTranscript("Where is the med kit?")).toBe(false);
  });

  it("adapts silence cutoff with speech duration", () => {
    expect(adaptiveSilenceOffMs(400, true)).toBeLessThan(adaptiveSilenceOffMs(3000, true));
    expect(adaptiveSilenceOffMs(1000, false)).toBe(900);
    expect(adaptiveSilenceOffMs(200, true)).toBeGreaterThanOrEqual(900);
  });

  it("speaks a soft clause when the stream has no period yet", () => {
    const spoken = extractFirstSpeakableSentence(
      "Sure, I can help with that right away once you confirm",
    );
    expect(typeof spoken).toBe("string");
    expect(spoken!.startsWith("Sure,")).toBe(true);
    expect(spoken!.length).toBeGreaterThan(20);
  });

  it("hardOnly early TTS waits for a real sentence end", () => {
    expect(
      extractFirstSpeakableSentence("Sure, I can help with that right away", 6, 200, {
        hardOnly: true,
      }),
    ).toBeNull();
    expect(
      extractFirstSpeakableSentence("Sure, I can help with that. More coming", 6, 200, {
        hardOnly: true,
      }),
    ).toBe("Sure, I can help with that.");
  });

  it("SpokenSentenceBuffer emits on punctuation + whitespace (nix-vox)", () => {
    const buf = new SpokenSentenceBuffer(12);
    expect(buf.push("Hello")).toEqual([]);
    expect(buf.push(".")).toEqual([]);
    // Next token starting with space confirms the prior sentence.
    expect(buf.push(" World")).toEqual(["Hello."]);
    expect(buf.push("!")).toEqual([]);
    expect(buf.push(" Next")).toEqual(["World!"]);
    expect(buf.flush()).toBe("Next");
  });

  it("SpokenSentenceBuffer flushes long run-ons by word count", () => {
    const buf = new SpokenSentenceBuffer(12);
    const words = "one two three four five six seven eight nine ten eleven twelve thirteen";
    const got = buf.push(words);
    expect(got.length).toBe(1);
    expect(got[0]).toContain("twelve");
  });
});
