/** Debounced main chat composer drafts (per chat, temp chat, or new-chat). */

export const CHAT_DRAFT_MAX_LEN = 32 * 1024;

const PREFIX = "athena-chat-draft-v1:";

export function chatDraftStorageKey(temporary: boolean, chatId: string | null): string {
  if (temporary) return `${PREFIX}temp`;
  if (chatId) return `${PREFIX}${chatId}`;
  return `${PREFIX}new`;
}

export function readChatDraft(temporary: boolean, chatId: string | null): string {
  try {
    const raw = localStorage.getItem(chatDraftStorageKey(temporary, chatId));
    return typeof raw === "string" ? raw.slice(0, CHAT_DRAFT_MAX_LEN) : "";
  } catch {
    return "";
  }
}

export function clearChatDraftFor(temporary: boolean, chatId: string | null) {
  try {
    localStorage.removeItem(chatDraftStorageKey(temporary, chatId));
  } catch {
    /* ignore */
  }
}
