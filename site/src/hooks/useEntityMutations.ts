import { useCallback } from 'react';
import { EntityDetail, EntityInput } from 'types/entity';
import { CREATE_ENTITY, DELETE_ENTITY, UPDATE_ENTITY } from './queries';
import { toastOnFailure } from './mutationToast';
import { REFETCH_AFTER_WRITE, useRefetchingMutation } from './useRefetchingMutation';

const REFETCH = [...REFETCH_AFTER_WRITE, 'GetEntity'];

/** Where an entity sits, as its create/update/delete needs to know. */
type Placed = Pick<EntityDetail, 'id' | 'parentId'>;

/** `create(input)` resolves to the new entity, or null after toasting a failure. */
export const useCreateEntity = () => {
  const [mutate, { loading }] = useRefetchingMutation<
    { createEntity: EntityDetail },
    { input: EntityInput }
  >(CREATE_ENTITY, {
    refetch: REFETCH,
    // The parent's cached contents no longer list everything it holds.
    evict: (_data, { input }) => [input.parentId],
  });
  const create = useCallback(
    (input: EntityInput) =>
      toastOnFailure(async () => (await mutate({ input })).createEntity, 'save', null),
    [mutate],
  );
  return { create, loading };
};

/**
 * `update(previous, input)` replaces every field of `previous.id` with
 * `input` (build it with `toEntityInput`) and resolves to the updated entity,
 * or null after toasting a failure. The parent is always evicted (a retype
 * moves the entity between its Locations and Items sections); on a move, the
 * old parent is evicted too.
 */
export const useUpdateEntity = () => {
  const [mutate, { loading }] = useRefetchingMutation<
    { updateEntity: EntityDetail },
    { id: string; input: EntityInput }
  >(UPDATE_ENTITY, { refetch: REFETCH });
  const update = useCallback(
    (previous: Placed, input: EntityInput) => {
      const parents =
        previous.parentId === input.parentId
          ? [input.parentId]
          : [previous.parentId, input.parentId];
      return toastOnFailure(
        async () => (await mutate({ id: previous.id, input }, parents)).updateEntity,
        'save',
        null,
      );
    },
    [mutate],
  );
  return { update, loading };
};

/** `remove(entity)` resolves true once deleted, or false after toasting a failure. */
export const useDeleteEntity = () => {
  const [mutate, { loading }] = useRefetchingMutation<{ deleteEntity: boolean }, { id: string }>(
    DELETE_ENTITY,
    { refetch: REFETCH_AFTER_WRITE, evict: (_data, { id }) => [id] },
  );
  const remove = useCallback(
    (entity: Placed) =>
      toastOnFailure(
        async () => (await mutate({ id: entity.id }, [entity.parentId])).deleteEntity,
        'delete',
        false,
      ),
    [mutate],
  );
  return { remove, loading };
};
