import { useQuery } from '@apollo/client/react';
import type { IngestBatch } from 'types/ingest';
import { GET_OPEN_INGEST_BATCHES } from './queries';
import { useErrorToast } from './useErrorToast';

const NO_BATCHES: IngestBatch[] = [];

/** The unfinished batches started from `parentId` (null: from no entity), newest first. */
export const useOpenIngestBatches = (parentId: string | null) => {
  const { data, loading, error } = useQuery<{ openIngestBatches: IngestBatch[] }>(
    GET_OPEN_INGEST_BATCHES,
    { variables: { parentId } },
  );
  useErrorToast(error, 'Error loading unfinished batches');
  return { batches: data?.openIngestBatches ?? NO_BATCHES, loading };
};
