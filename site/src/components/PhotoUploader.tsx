import clsx from 'clsx';
import { ChangeEvent, DragEvent, ReactNode, useRef, useState } from 'react';
import { toast } from 'react-toastify';
import { UploadProgress, UploadStatus, useUploadPhoto } from 'hooks/useUploadPhoto';
import { SECONDARY_ACTION } from './buttonStyles';

/** The formats the server accepts (it sniffs the bytes; this only filters the picker). */
const ACCEPTED_TYPES = 'image/jpeg,image/png,image/gif,image/webp';

const STATUS_TEXT: Record<Exclude<UploadStatus, 'error'>, string> = {
  queued: 'Waiting',
  uploading: 'Uploading',
  done: 'Uploaded',
};

const StatusRow = ({ entry }: { entry: UploadProgress }) => (
  <li className="flex min-w-0 flex-wrap items-baseline gap-x-2 text-sm">
    <span className="min-w-0 truncate text-text">{entry.file.name}</span>
    {entry.status === 'error' ? (
      <span className="text-danger">{entry.error}</span>
    ) : (
      <span className={entry.status === 'done' ? 'text-success' : 'text-muted'}>
        {STATUS_TEXT[entry.status]}
      </span>
    )}
  </li>
);

/**
 * Adds photos to `entity`: an `Add photos` button over a hidden multi-file
 * image input, and the whole region (wrapped around `children`, the gallery)
 * as a drop target. The latest batch's per-file status is listed below. Files
 * dropped while a batch is still uploading are refused with a toast. Pages key
 * it by entity id so its status never follows navigation to another entity.
 */
export const PhotoUploader = ({
  entity,
  children,
}: {
  entity: { id: string; parentId: string | null };
  children?: ReactNode;
}) => {
  const { upload, uploading, progress } = useUploadPhoto(entity);
  const inputRef = useRef<HTMLInputElement>(null);
  const [dragging, setDragging] = useState(false);

  const send = (files: File[]) => {
    if (files.length === 0) return;
    // One batch at a time keeps the server's first-photo/primary order deterministic.
    if (uploading) {
      toast.error('Wait for the current upload to finish');
      return;
    }
    void upload(files);
  };

  const onChoose = (event: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.target.files ?? []);
    // Cleared, so picking the same file again still fires a change.
    event.target.value = '';
    send(files);
  };

  const onDragOver = (event: DragEvent<HTMLDivElement>) => {
    if (!event.dataTransfer.types.includes('Files')) return;
    event.preventDefault();
    setDragging(true);
  };

  const onDragLeave = (event: DragEvent<HTMLDivElement>) => {
    if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDragging(false);
  };

  const onDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    setDragging(false);
    send(Array.from(event.dataTransfer.files));
  };

  return (
    <div
      role="group"
      aria-label="Photo upload"
      data-dragging={dragging || undefined}
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDrop}
      className={clsx(
        'rounded-lg border border-dashed p-3',
        dragging ? 'border-accent bg-surface-raised' : 'border-border',
      )}
    >
      <div className="mb-3 flex flex-wrap items-center gap-3">
        <button
          type="button"
          onClick={() => inputRef.current?.click()}
          disabled={uploading}
          className={SECONDARY_ACTION}
        >
          {uploading ? 'Uploading…' : 'Add photos'}
        </button>
        <span className="text-sm text-muted">or drop images here (JPEG, PNG, GIF, WebP)</span>
        <input
          ref={inputRef}
          type="file"
          accept={ACCEPTED_TYPES}
          multiple
          aria-label="Photo files"
          tabIndex={-1}
          disabled={uploading}
          onChange={onChoose}
          className="sr-only"
        />
      </div>
      {children}
      {progress.length > 0 && (
        <ul aria-label="Upload status" aria-live="polite" className="mt-3 space-y-1">
          {progress.map((entry, index) => (
            <StatusRow key={`${index}-${entry.file.name}`} entry={entry} />
          ))}
        </ul>
      )}
    </div>
  );
};
