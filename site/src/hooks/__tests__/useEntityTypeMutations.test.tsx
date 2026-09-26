import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act } from '@testing-library/react';
import {
  useCreateEntityType,
  useDeleteEntityType,
  useUpdateEntityType,
} from '../useEntityTypeMutations';
import { CREATE_ENTITY_TYPE, DELETE_ENTITY_TYPE, UPDATE_ENTITY_TYPE } from '../queries';
import { REFETCHED_SUMMARY, setupWithSummary } from 'test/mutationHarness';
import { EntityTypeInput } from 'types/entity';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const input: EntityTypeInput = { name: 'Bin', description: null, icon: null, isLocation: true };
const bin = { __typename: 'EntityType', id: 'bin', ...input, entityCount: 0 };

describe('entity type mutations', () => {
  it('create returns the type and refetches the summary', async () => {
    const { result, refetched } = await setupWithSummary(useCreateEntityType, [
      {
        request: { query: CREATE_ENTITY_TYPE, variables: { input } },
        result: { data: { createEntityType: bin } },
      },
    ]);
    let created: unknown;
    await act(async () => {
      created = await result.current.hook.create(input);
    });
    expect(created).toEqual(bin);
    expect(refetched).toHaveBeenCalledTimes(1);
    expect(result.current.summary).toEqual(REFETCHED_SUMMARY);
  });

  it('update returns the type and refetches the summary', async () => {
    const renamed = { ...input, name: 'Crate' };
    const { result, refetched } = await setupWithSummary(useUpdateEntityType, [
      {
        request: { query: UPDATE_ENTITY_TYPE, variables: { id: 'bin', input: renamed } },
        result: { data: { updateEntityType: { ...bin, name: 'Crate' } } },
      },
    ]);
    let updated: unknown;
    await act(async () => {
      updated = await result.current.hook.update('bin', renamed);
    });
    expect(updated).toEqual({ ...bin, name: 'Crate' });
    expect(refetched).toHaveBeenCalledTimes(1);
  });

  it('update toasts "Could not save" on failure', async () => {
    const { result } = await setupWithSummary(useUpdateEntityType, [
      {
        request: { query: UPDATE_ENTITY_TYPE, variables: { id: 'bin', input } },
        result: { errors: [{ message: 'type has location children' }] },
      },
    ]);
    let updated: unknown;
    await act(async () => {
      updated = await result.current.hook.update('bin', input);
    });
    expect(updated).toBeNull();
    expect(toast.error).toHaveBeenCalledWith('Could not save: type has location children');
  });

  it('delete resolves true and refetches the summary', async () => {
    const { result, refetched } = await setupWithSummary(useDeleteEntityType, [
      {
        request: { query: DELETE_ENTITY_TYPE, variables: { id: 'bin' } },
        result: { data: { deleteEntityType: true } },
      },
    ]);
    let deleted: unknown;
    await act(async () => {
      deleted = await result.current.hook.remove('bin');
    });
    expect(deleted).toBe(true);
    expect(refetched).toHaveBeenCalledTimes(1);
  });

  it('delete toasts "Could not delete" on failure', async () => {
    const { result } = await setupWithSummary(useDeleteEntityType, [
      {
        request: { query: DELETE_ENTITY_TYPE, variables: { id: 'item' } },
        result: { errors: [{ message: 'Item is used by 4 entities' }] },
      },
    ]);
    let deleted: unknown;
    await act(async () => {
      deleted = await result.current.hook.remove('item');
    });
    expect(deleted).toBe(false);
    expect(toast.error).toHaveBeenCalledWith('Could not delete: Item is used by 4 entities');
  });
});
