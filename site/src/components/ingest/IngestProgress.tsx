import clsx from 'clsx';
import type { IngestBatch, IngestItemStatus } from 'types/ingest';

const STATUS_LABEL: Record<IngestItemStatus, string> = {
  // Submitting drops empty items, so a collecting item never reaches this strip.
  COLLECTING: 'Queued',
  QUEUED: 'Queued',
  ANALYSING: 'Analysing',
  READY: 'Ready',
  FAILED: 'Failed',
  ACCEPTED: 'Saved',
  SKIPPED: 'Skipped',
};

const STATUS_CLASS: Record<IngestItemStatus, string> = {
  COLLECTING: 'text-muted',
  QUEUED: 'text-muted',
  ANALYSING: 'text-text',
  READY: 'text-accent',
  FAILED: 'text-danger',
  ACCEPTED: 'text-success',
  SKIPPED: 'text-muted',
};

/**
 * One chip per item with where it stands, announced politely as the batch
 * refetches; the item under review (`currentId`) is marked as the current step.
 */
export const IngestProgress = ({
  batch,
  currentId,
}: {
  batch: IngestBatch;
  currentId?: string | null;
}) => (
  <ul aria-label="Progress" aria-live="polite" className="mt-6 flex flex-wrap gap-2">
    {batch.items.map((item, index) => (
      <li
        key={item.id}
        aria-current={item.id === currentId ? 'step' : undefined}
        className={clsx(
          'rounded-full border px-3 py-1 text-sm',
          item.id === currentId ? 'border-accent bg-surface-raised' : 'border-border bg-surface',
          STATUS_CLASS[item.status],
        )}
      >
        Item {index + 1}: {STATUS_LABEL[item.status]}
      </li>
    ))}
  </ul>
);
