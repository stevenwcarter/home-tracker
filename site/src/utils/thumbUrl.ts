export type ThumbSize = 300 | 500 | 1200;

/**
 * Rewrites the `/thumb/<n>` segment of a thumbnail URL to `size`, keeping
 * everything else (notably the `?v=` cache-buster) intact. A URL without the
 * segment is returned unchanged.
 */
export function thumbUrlAt(url: string, size: ThumbSize): string {
  return url.replace(/\/thumb\/\d+(?=$|[?#/])/, `/thumb/${size}`);
}
