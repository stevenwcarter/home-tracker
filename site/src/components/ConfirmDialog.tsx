import { KeyboardEvent, ReactNode, useEffect, useId, useRef } from 'react';

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  body: ReactNode;
  /** The confirm button's text, naming the action ("Delete item"). */
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
  /** Disables both buttons while the confirmed action runs. */
  busy?: boolean;
}

/**
 * A modal yes/no prompt (the app's replacement for `window.confirm`). Opening
 * it moves focus to Cancel, the safe choice; Tab stays inside it; Escape and a
 * backdrop click cancel; closing returns focus to whatever had it before.
 */
export const ConfirmDialog = ({
  open,
  title,
  body,
  confirmLabel,
  onConfirm,
  onCancel,
  busy = false,
}: ConfirmDialogProps) => {
  const titleId = useId();
  const bodyId = useId();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);
  // Read through a ref so a new `onCancel` identity each render doesn't re-run the effect.
  const onCancelRef = useRef(onCancel);
  useEffect(() => {
    onCancelRef.current = onCancel;
  }, [onCancel]);

  useEffect(() => {
    if (!open) return;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    cancelRef.current?.focus();
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onCancelRef.current();
      }
    };
    // Capture phase, so this runs before bubble-phase Escape handlers (the
    // drawer's), which skip the event once it is marked handled here.
    document.addEventListener('keydown', onKeyDown, { capture: true });
    return () => {
      document.removeEventListener('keydown', onKeyDown, { capture: true });
      opener?.focus();
    };
  }, [open]);

  if (!open) return null;

  // With only two focusable controls, Tab and Shift+Tab both just swap between them.
  const trapTab = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'Tab') return;
    event.preventDefault();
    const next =
      document.activeElement === cancelRef.current ? confirmRef.current : cancelRef.current;
    next?.focus();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <button
        type="button"
        tabIndex={-1}
        aria-label="Close dialog"
        data-testid="confirm-dialog-backdrop"
        onClick={onCancel}
        className="absolute inset-0 bg-bg/70"
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={bodyId}
        onKeyDown={trapTab}
        className="relative w-full max-w-md rounded-lg border border-border bg-surface p-5 shadow-lg"
      >
        <h2 id={titleId} className="text-lg font-semibold text-text">
          {title}
        </h2>
        <div id={bodyId} className="mt-2 text-sm text-muted">
          {body}
        </div>
        <div className="mt-5 flex justify-end gap-3">
          <button
            ref={cancelRef}
            type="button"
            onClick={onCancel}
            disabled={busy}
            className="rounded-md border border-border px-3 py-1.5 text-sm text-text hover:bg-surface-raised disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            ref={confirmRef}
            type="button"
            onClick={onConfirm}
            disabled={busy}
            className="rounded-md bg-danger px-3 py-1.5 text-sm font-medium text-accent-text disabled:opacity-50"
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
};
