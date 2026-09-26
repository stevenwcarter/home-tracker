import { useEffect, useRef } from 'react';
import { useQuery } from '@apollo/client/react';
import type { IngestBatch } from 'types/ingest';
import { GET_INGEST_BATCH } from './queries';
import { useErrorToast } from './useErrorToast';

/** How long a burst of progress events waits before its one refetch. */
export const REFETCH_DEBOUNCE_MS = 150;

/**
 * The server-sent events that mean the batch changed, plus `open`: a
 * (re)connect may have missed events, so it refetches the snapshot too.
 */
const REFETCH_ON = ['open', 'photo', 'item', 'batch'] as const;

interface IngestBatchResponse {
  ingestBatch: IngestBatch | null;
}

/**
 * Batch `id`, kept live: runs `GetIngestBatch` and, while the batch is not
 * `DONE`, listens to `GET /api/ingest/batches/{id}/events`. Every event (and
 * every `open`, so a reconnect catches up) schedules a refetch, debounced by
 * {@link REFETCH_DEBOUNCE_MS} so a burst costs one query. The stream closes on
 * unmount, on an id change and once the batch is done (the server ends a done
 * batch's stream, and the browser would otherwise reconnect to it forever);
 * a dropped connection is left to the browser's own reconnect.
 */
export const useIngestBatch = (id: string) => {
  const { data, loading, error, refetch } = useQuery<IngestBatchResponse>(GET_INGEST_BATCH, {
    variables: { id },
  });
  useErrorToast(error, 'Error loading the batch');
  const batch = data?.ingestBatch ?? null;
  const live = batch?.status !== 'DONE';

  // Read through a ref, so a new `refetch` identity doesn't reopen the stream.
  const refetchRef = useRef(refetch);
  useEffect(() => {
    refetchRef.current = refetch;
  }, [refetch]);

  useEffect(() => {
    if (!live || typeof EventSource === 'undefined') return;
    const source = new EventSource(`/api/ingest/batches/${encodeURIComponent(id)}/events`);
    let timer: ReturnType<typeof setTimeout> | undefined;
    const schedule = () => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        // A failed refetch surfaces as the query's own error (toasted above).
        refetchRef.current().catch(() => {});
      }, REFETCH_DEBOUNCE_MS);
    };
    for (const type of REFETCH_ON) source.addEventListener(type, schedule);
    return () => {
      clearTimeout(timer);
      for (const type of REFETCH_ON) source.removeEventListener(type, schedule);
      source.close();
    };
  }, [id, live]);

  const notFound = !loading && !error && data !== undefined && data.ingestBatch === null;
  return { batch, loading, error, refetch, notFound };
};
