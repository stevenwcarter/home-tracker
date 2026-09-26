import { useCallback } from 'react';
import { TagDetail, TagInput } from 'types/entity';
import { CREATE_TAG, DELETE_TAG, UPDATE_TAG } from './queries';
import { toastOnFailure } from './mutationToast';
import { REFETCH_AFTER_WRITE, useRefetchingMutation } from './useRefetchingMutation';
import { TagWire, toTagDetail } from './useTags';

// An entity page shows its tags, so an open one refetches too.
const REFETCH = [...REFETCH_AFTER_WRITE, 'GetEntity'];

/** `create(input)` resolves to the new tag, or null after toasting a failure. */
export const useCreateTag = () => {
  const [mutate, { loading }] = useRefetchingMutation<{ createTag: TagWire }, { input: TagInput }>(
    CREATE_TAG,
    { refetch: REFETCH },
  );
  const create = useCallback(
    (input: TagInput): Promise<TagDetail | null> =>
      toastOnFailure(async () => toTagDetail((await mutate({ input })).createTag), 'save', null),
    [mutate],
  );
  return { create, loading };
};

/** `update(id, input)` replaces every field; resolves to the tag, or null after toasting a failure. */
export const useUpdateTag = () => {
  const [mutate, { loading }] = useRefetchingMutation<
    { updateTag: TagWire },
    { id: string; input: TagInput }
  >(UPDATE_TAG, { refetch: REFETCH });
  const update = useCallback(
    (id: string, input: TagInput): Promise<TagDetail | null> =>
      toastOnFailure(
        async () => toTagDetail((await mutate({ id, input })).updateTag),
        'save',
        null,
      ),
    [mutate],
  );
  return { update, loading };
};

/** `remove(id)` resolves true once deleted, or false after toasting a failure. */
export const useDeleteTag = () => {
  const [mutate, { loading }] = useRefetchingMutation<{ deleteTag: boolean }, { id: string }>(
    DELETE_TAG,
    { refetch: REFETCH },
  );
  const remove = useCallback(
    (id: string) => toastOnFailure(async () => (await mutate({ id })).deleteTag, 'delete', false),
    [mutate],
  );
  return { remove, loading };
};
