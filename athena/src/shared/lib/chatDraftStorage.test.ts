import { afterEach, describe, expect, it } from "vitest";
import {
  CHAT_DRAFT_MAX_LEN,
  chatDraftStorageKey,
  clearChatDraftFor,
  readChatDraft,
} from "./chatDraftStorage";

describe("chatDraftStorage", () => {
  afterEach(() => {
    localStorage.clear();
  });

  it("uses distinct keys for temp, new, and id", () => {
    expect(chatDraftStorageKey(true, null)).toBe("athena-chat-draft-v1:temp");
    expect(chatDraftStorageKey(false, null)).toBe("athena-chat-draft-v1:new");
    expect(chatDraftStorageKey(false, "abc")).toBe("athena-chat-draft-v1:abc");
  });

  it("round-trips draft text capped to max length", () => {
    const long = "x".repeat(CHAT_DRAFT_MAX_LEN + 50);
    localStorage.setItem(chatDraftStorageKey(false, "c1"), long);
    expect(readChatDraft(false, "c1").length).toBe(CHAT_DRAFT_MAX_LEN);
  });

  it("clearChatDraftFor removes the key", () => {
    localStorage.setItem(chatDraftStorageKey(false, null), "hello");
    clearChatDraftFor(false, null);
    expect(readChatDraft(false, null)).toBe("");
  });
});
