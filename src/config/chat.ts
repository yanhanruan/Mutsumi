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

/**
 * How many transcript messages to load per page — for the initial open, for
 * each upward (older) / downward (newer) scroll fetch, and for the window loaded
 * when jumping to a History search result. A short page (< this) signals an edge
 * of the timeline has been reached.
 */
export const CHAT_HISTORY_PAGE = 30

/**
 * Time-grouping threshold (seconds) for the chat thread. A message starts a new
 * group — and shows a `mm-dd hh:mm` time separator — when the gap from the
 * previous message exceeds this. The group's label is its first message's time.
 */
export const CHAT_TIME_GROUP_GAP = 300  // 5 minutes

/** Image input limits. Size (≤5 MB) is enforced in Rust; the frontend gates count. */
export const IMAGE_MAX_COUNT = 3
export const IMAGE_EXT_WHITELIST = [
  'bmp', 'jpe', 'jpeg', 'jpg', 'png', 'tif', 'tiff', 'webp', 'heic',
]

/** A persisted transcript row, as returned by the `chat_*` history commands. */
export interface StoredMessage {
  id: number
  role: 'user' | 'assistant'
  content: string
  created_at: number   // unix seconds
  kind: 'text' | 'image'
  image_path: string | null   // absolute path (resolved by Rust) for image rows
}
