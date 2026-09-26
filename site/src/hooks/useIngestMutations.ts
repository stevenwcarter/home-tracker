import { useCallback, useMemo } from 'react';
import type { DocumentNode } from '@apollo/client';
import type { EntityDetail, EntityInput } from 'types/entity';
import type { IngestBatch, IngestItem, IngestPhotoKindInput } from 'types/ingest';
import {
  ACCEPT_INGEST_ITEM,
  ADD_INGEST_ITEM,
  CREATE_INGEST_BATCH,
  DELETE_INGEST_BATCH,
  REMOVE_INGEST_ITEM,
  REMOVE_INGEST_PHOTO,
  RETRY_INGEST_ITEM,
  SKIP_INGEST_ITEM,
  SUBMIT_INGEST_BATCH,
} from './queries';
import { FailureVerb, toastOnFailure } from './mutationToast';
import { REFETCH_AFTER_WRITE, useRefetchingMutation } from './useRefetchingMutation';

/** Every ingest write can change the open batch and a parent's list of unfinished ones. */
const INGEST_REFETCH = ['GetIngestBatch', 'GetOpenIngestBatches'];

/** Accepting also creates an entity, so it adds the entity refetch policy. */
const ACCEPT_REFETCH = [...REFETCH_AFTER_WRITE, 'GetEntity', ...INGEST_REFETCH];

/**
 * An ingest mutation taking one id variable (named `variable`) and answering
 * `field`: `run(id)` resolves to the answer, or to `fallback` after toasting
 * `Could not <verb>: <message>`.
 */
const useIdMutation = <T, F, Id extends string | null = string>(
  mutation: DocumentNode,
  field: string,
  variable: string,
  verb: FailureVerb,
  fallback: F,
) => {
  const [mutate, { loading }] = useRefetchingMutation<
    Record<string, T>,
    Record<string, string | null>
  >(mutation, { refetch: INGEST_REFETCH });
  const run = useCallback(
    (id: Id): Promise<T | F> =>
      toastOnFailure(async () => (await mutate({ [variable]: id }))[field], verb, fallback),
    [mutate, field, variable, verb, fallback],
  );
  return [run, loading] as const;
};

/**
 * The AI ingest writes. Each resolves to the server's answer, or (after
 * toasting "Could not ...") to null for a record and false for a boolean,
 * and refetches `GetIngestBatch` and `GetOpenIngestBatches` when mounted
 * (evicting them when not).
 *
 * `acceptItem(id, input, kinds)` creates the entity from the user's `input`
 * (the full `EntityInput`) with each staged photo stored as its chosen kind,
 * resolves to the new entity, and also applies the entity refetch policy,
 * evicting `input.parentId`, the parent the user chose, whose contents grew.
 */
export const useIngestMutations = () => {
  const [createBatch, creating] = useIdMutation<IngestBatch, null, string | null>(
    CREATE_INGEST_BATCH,
    'createIngestBatch',
    'parentId',
    'start the batch',
    null,
  );
  const [addItem, adding] = useIdMutation<IngestItem, null>(
    ADD_INGEST_ITEM,
    'addIngestItem',
    'batchId',
    'add an item',
    null,
  );
  const [removeItem, removingItem] = useIdMutation<boolean, false>(
    REMOVE_INGEST_ITEM,
    'removeIngestItem',
    'id',
    'remove the item',
    false,
  );
  const [removePhoto, removingPhoto] = useIdMutation<boolean, false>(
    REMOVE_INGEST_PHOTO,
    'removeIngestPhoto',
    'id',
    'remove the photo',
    false,
  );
  const [submit, submitting] = useIdMutation<IngestBatch, null>(
    SUBMIT_INGEST_BATCH,
    'submitIngestBatch',
    'id',
    'submit the batch',
    null,
  );
  const [retryItem, retrying] = useIdMutation<IngestItem, null>(
    RETRY_INGEST_ITEM,
    'retryIngestItem',
    'id',
    'retry',
    null,
  );
  const [skipItem, skipping] = useIdMutation<IngestItem, null>(
    SKIP_INGEST_ITEM,
    'skipIngestItem',
    'id',
    'skip',
    null,
  );
  const [deleteBatch, deleting] = useIdMutation<boolean, false>(
    DELETE_INGEST_BATCH,
    'deleteIngestBatch',
    'id',
    'delete',
    false,
  );

  const [accept, { loading: accepting }] = useRefetchingMutation<
    { acceptIngestItem: EntityDetail },
    { id: string; input: EntityInput; photoKinds: IngestPhotoKindInput[] }
  >(ACCEPT_INGEST_ITEM, {
    refetch: ACCEPT_REFETCH,
    // The chosen parent's cached contents no longer list everything it holds.
    evict: (_data, { input }) => [input.parentId],
  });
  const acceptItem = useCallback(
    (id: string, input: EntityInput, kinds: IngestPhotoKindInput[]) =>
      toastOnFailure(
        async () => (await accept({ id, input, photoKinds: kinds })).acceptIngestItem,
        'save',
        null,
      ),
    [accept],
  );

  const loading =
    creating ||
    adding ||
    removingItem ||
    removingPhoto ||
    submitting ||
    retrying ||
    skipping ||
    deleting ||
    accepting;

  return useMemo(
    () => ({
      createBatch,
      addItem,
      removeItem,
      removePhoto,
      submit,
      retryItem,
      acceptItem,
      skipItem,
      deleteBatch,
      loading,
    }),
    [
      createBatch,
      addItem,
      removeItem,
      removePhoto,
      submit,
      retryItem,
      acceptItem,
      skipItem,
      deleteBatch,
      loading,
    ],
  );
};
