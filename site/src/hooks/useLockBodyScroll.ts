import { useEffect } from 'react';

// How many mounted holders want the lock, and the overflow to restore when the last lets go.
let holders = 0;
let restore = '';

/**
 * Stops the page behind a modal (`Lightbox`, `ConfirmDialog`) from scrolling
 * while `active`, by hiding `body` overflow. Overlapping holders share one
 * lock; the body's own overflow comes back when the last one releases.
 */
export function useLockBodyScroll(active: boolean): void {
  useEffect(() => {
    if (!active) return;
    if (holders === 0) {
      restore = document.body.style.overflow;
      document.body.style.overflow = 'hidden';
    }
    holders += 1;
    return () => {
      holders -= 1;
      if (holders === 0) document.body.style.overflow = restore;
    };
  }, [active]);
}
