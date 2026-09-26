import { useCallback } from 'react';
import { EntityTypeDetail, EntityTypeInput } from 'types/entity';
import { CREATE_ENTITY_TYPE, DELETE_ENTITY_TYPE, UPDATE_ENTITY_TYPE } from './queries';
import { toastOnFailure } from './mutationToast';
import { REFETCH_AFTER_WRITE, useRefetchingMutation } from './useRefetchingMutation';

// An entity page shows its type's name, so an open one refetches too.
const REFETCH = [...REFETCH_AFTER_WRITE, 'GetEntity'];

/** `create(input)` resolves to the new type, or null after toasting a failure. */
export const useCreateEntityType = () => {
  const [mutate, { loading }] = useRefetchingMutation<
    { createEntityType: EntityTypeDetail },
    { input: EntityTypeInput }
  >(CREATE_ENTITY_TYPE, { refetch: REFETCH });
  const create = useCallback(
    (input: EntityTypeInput) =>
      toastOnFailure(async () => (await mutate({ input })).createEntityType, 'save', null),
    [mutate],
  );
  return { create, loading };
};

/** `update(id, input)` replaces every field; resolves to the type, or null after toasting a failure. */
export const useUpdateEntityType = () => {
  const [mutate, { loading }] = useRefetchingMutation<
    { updateEntityType: EntityTypeDetail },
    { id: string; input: EntityTypeInput }
  >(UPDATE_ENTITY_TYPE, { refetch: REFETCH });
  const update = useCallback(
    (id: string, input: EntityTypeInput) =>
      toastOnFailure(async () => (await mutate({ id, input })).updateEntityType, 'save', null),
    [mutate],
  );
  return { update, loading };
};

/** `remove(id)` resolves true once deleted, or false after toasting a failure (e.g. type in use). */
export const useDeleteEntityType = () => {
  const [mutate, { loading }] = useRefetchingMutation<
    { deleteEntityType: boolean },
    { id: string }
  >(DELETE_ENTITY_TYPE, { refetch: REFETCH });
  const remove = useCallback(
    (id: string) =>
      toastOnFailure(async () => (await mutate({ id })).deleteEntityType, 'delete', false),
    [mutate],
  );
  return { remove, loading };
};
