/**
 * The largest file the server stores: 25 MiB, `MAX_UPLOAD_BYTES` in
 * `src/api/upload.rs`. Checked before posting, because the server refuses an
 * oversized body before reading it and a browser then sees only a reset
 * connection, never the 413.
 */
export const MAX_UPLOAD_BYTES = 25 * 1024 * 1024;

/** The message for a file over {@link MAX_UPLOAD_BYTES}, also the 413's. */
const TOO_LARGE_MESSAGE = 'File is larger than 25 MB';

/** The upload statuses whose message is fixed here rather than taken from the server. */
const FIXED_MESSAGES: Readonly<Record<number, string>> = {
  413: TOO_LARGE_MESSAGE,
  415: 'Only JPEG, PNG, GIF and WebP images are supported',
  422: 'That file is not a readable image',
};

/** Shown when the request never got an answer (offline, server down). */
export const NETWORK_ERROR_MESSAGE = 'Could not reach the server';

/**
 * The message for a failed `POST /api/upload/{id}`: a fixed, friendly one for
 * the too-big / wrong-format / unreadable statuses, else the server's own
 * `{ "error": ... }` text, else the bare status.
 */
export function uploadErrorMessage(status: number, serverMessage: string | null): string {
  return FIXED_MESSAGES[status] ?? (serverMessage || `Upload failed (HTTP ${status})`);
}

/** Why `file` would be refused before it is sent, or null when it may be posted. */
export const preflightError = (file: File): string | null =>
  file.size > MAX_UPLOAD_BYTES ? TOO_LARGE_MESSAGE : null;
