import { KeyboardEvent, useEffect, useId, useRef, useState } from 'react';
import { useLockBodyScroll } from 'hooks/useLockBodyScroll';
import { AttachmentRef } from 'types/entity';
import { thumbUrlAt } from 'utils/thumbUrl';
import { SECONDARY_ACTION } from './buttonStyles';

interface LightboxProps {
  /** The photos to step through; each must have a thumbnail. */
  photos: AttachmentRef[];
  /** The photo shown first. */
  startIndex: number;
  onClose: () => void;
  /**
   * Where focus goes back to on close (the thumbnail that opened it). Passed
   * in because Safari does not focus a button on mouse click; without it, the
   * element focused at open time is used.
   */
  opener?: HTMLElement | null;
}

const FOCUSABLE = 'button:not([disabled]), a[href]';

/**
 * A full-screen photo viewer: the 1200 thumbnail, sized to fit the viewport,
 * with Previous/Next (wrapping at the ends; the arrow keys too) and a link to
 * the original. Opening it moves focus to Close; Tab stays inside it; Escape
 * and a backdrop click close it; closing returns focus to whatever had it
 * before (the gallery thumbnail that opened it).
 */
export const Lightbox = ({ photos, startIndex, onClose, opener }: LightboxProps) => {
  const [index, setIndex] = useState(startIndex);
  const titleId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  // Captured at open, like the fallback below; a later prop change does not move it.
  const openerRef = useRef(opener);
  useLockBodyScroll(true);
  const count = photos.length;
  // Clamped, so a photo deleted from under an open viewer shows its neighbour.
  const shown = Math.min(index, count - 1);
  const current = photos[shown];
  const step = (by: number) =>
    setIndex((at) => (((Math.min(at, count - 1) + by) % count) + count) % count);
  // Read through refs so new callback identities don't re-run the mount effect.
  const onCloseRef = useRef(onClose);
  const stepRef = useRef(step);
  useEffect(() => {
    onCloseRef.current = onClose;
    stepRef.current = step;
  });

  useEffect(() => {
    const returnTo =
      openerRef.current ??
      (document.activeElement instanceof HTMLElement ? document.activeElement : null);
    closeRef.current?.focus();
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.defaultPrevented) return;
      if (event.key === 'Escape') {
        event.preventDefault();
        onCloseRef.current();
      } else if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
        event.preventDefault();
        stepRef.current(event.key === 'ArrowRight' ? 1 : -1);
      }
    };
    // Capture phase, so this runs before bubble-phase Escape handlers (the
    // drawer's), which skip the event once it is marked handled here.
    document.addEventListener('keydown', onKeyDown, { capture: true });
    return () => {
      document.removeEventListener('keydown', onKeyDown, { capture: true });
      returnTo?.focus();
    };
  }, []);

  if (!current?.thumbnailUrl) return null;

  const trapTab = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'Tab' || !dialogRef.current) return;
    const focusable = [...dialogRef.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-2 sm:p-4">
      <button
        type="button"
        tabIndex={-1}
        aria-label="Close viewer"
        data-testid="lightbox-backdrop"
        onClick={onClose}
        className="absolute inset-0 bg-bg/90"
      />
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onKeyDown={trapTab}
        className="relative flex max-h-full w-full max-w-5xl flex-col gap-3 rounded-lg border border-border bg-surface p-3 shadow-lg"
      >
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h2 id={titleId} className="min-w-0 truncate font-semibold text-text">
            {current.title}
          </h2>
          <div className="flex items-center gap-2">
            <a href={current.url} className="text-sm text-accent hover:underline">
              Open original
            </a>
            <button ref={closeRef} type="button" onClick={onClose} className={SECONDARY_ACTION}>
              Close
            </button>
          </div>
        </div>
        <img
          src={thumbUrlAt(current.thumbnailUrl, 1200)}
          alt={current.title}
          className="mx-auto max-h-[75vh] min-h-0 w-auto max-w-full rounded-md bg-surface-raised object-contain"
        />
        {count > 1 && (
          <div className="flex items-center justify-between gap-2">
            <button
              type="button"
              onClick={() => step(-1)}
              aria-label="Previous photo"
              className={SECONDARY_ACTION}
            >
              Previous
            </button>
            <span className="text-sm text-muted">
              {shown + 1} of {count}
            </span>
            <button
              type="button"
              onClick={() => step(1)}
              aria-label="Next photo"
              className={SECONDARY_ACTION}
            >
              Next
            </button>
          </div>
        )}
      </div>
    </div>
  );
};
