/** The upload statuses whose message is fixed here rather than taken from the server. */
const FIXED_MESSAGES: Readonly<Record<number, string>> = {
  413: 'File is larger than 25 MB',
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
