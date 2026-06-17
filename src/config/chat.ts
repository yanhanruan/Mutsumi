/**
 * chat.ts — Chat overlay configuration.
 *
 * Mirrors the tarot overlay's window-sizing approach (see config/tarot.ts):
 * while the chat panel is open the main window grows to a conversation-suitable
 * size, scaled by the 大中小 (small / medium / large) character setting, and
 * restores to CHAR_SIZE_DIMS on close.
 */

/**
 * Main-window size (logical px) while the chat overlay is open, per character
 * size tier. A chat panel wants more height than the tarot card for the message
 * history + input row.
 */
export const CHAT_WINDOW_DIMS: Record<'small' | 'medium' | 'large', [number, number]> = {
  small:  [320, 480],
  medium: [360, 540],
  large:  [400, 600],
}

/** Cap on the conversation history (message pairs) kept in memory / sent back. */
export const CHAT_MAX_HISTORY = 12
