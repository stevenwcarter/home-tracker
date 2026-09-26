import { useState } from 'react';
import type { IngestBatch, IngestItem } from 'types/ingest';
import { pinnedReviewable } from 'utils/ingest';

/**
 * The item under review, pinned across refetches: it stays the same while
 * that item is still `READY` or `FAILED`, so another item turning ready (or
 * a retried one coming back) never swaps the form out mid-edit. Once it is
 * accepted, skipped or queued again, the next reviewable item takes over.
 */
export const useReviewedItem = (batch: Pick<IngestBatch, 'items'>): IngestItem | null => {
  const [pinnedId, setPinnedId] = useState<string | null>(null);
  const item = pinnedReviewable(batch, pinnedId);
  const id = item?.id ?? null;
  // Adjusting state while rendering: React re-renders at once, before
  // committing, with the same item.
  if (id !== pinnedId) setPinnedId(id);
  return item;
};
