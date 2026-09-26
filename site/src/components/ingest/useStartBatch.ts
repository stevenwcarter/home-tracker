import { useCallback, useRef } from 'react';
import { useNavigate } from 'react-router-dom';
import { useIngestMutations } from 'hooks/useIngestMutations';

/**
 * `start(parentId)` creates a batch under `parentId` and opens it; on failure
 * the mutation hook has toasted and the page stays put. A second call while
 * one is in flight is ignored, so a double tap starts one batch, not two
 * (the mutation's `loading` only disables the button a render later).
 */
export const useStartBatch = () => {
  const { createBatch, loading } = useIngestMutations();
  const navigate = useNavigate();
  const inFlight = useRef(false);
  const start = useCallback(
    async (parentId: string | null) => {
      if (inFlight.current) return;
      inFlight.current = true;
      try {
        const batch = await createBatch(parentId);
        if (batch) navigate(`/ingest/${batch.id}`);
      } finally {
        inFlight.current = false;
      }
    },
    [createBatch, navigate],
  );
  return { start, starting: loading };
};
