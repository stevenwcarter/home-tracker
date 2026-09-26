import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act } from '@testing-library/react';
import type { MockedResponse } from '@apollo/client/testing';
import { useCreateEntity, useDeleteEntity, useUpdateEntity } from '../useEntityMutations';
import { CREATE_ENTITY, DELETE_ENTITY, UPDATE_ENTITY } from '../queries';
import { entityDetail } from 'test/entityFixtures';
import { REFETCHED_SUMMARY, setupWithSummary } from 'test/mutationHarness';
import { toEntityInput } from 'utils/entityInput';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const drill = entityDetail({ id: 'drill', name: 'Drill', parentId: 'garage' });
const input = toEntityInput(drill);

const failing = (query: MockedResponse['request']['query'], variables: object) => ({
  request: { query, variables },
  result: { errors: [{ message: 'name must not be blank' }] },
});

describe('useCreateEntity', () => {
  it('creates, returns the entity and refetches the summary', async () => {
    const { result, refetched } = await setupWithSummary(useCreateEntity, [
      {
        request: { query: CREATE_ENTITY, variables: { input } },
        result: { data: { createEntity: drill } },
      },
    ]);
    let created: unknown;
    await act(async () => {
      created = await result.current.hook.create(input);
    });
    expect(created).toEqual(drill);
    expect(refetched).toHaveBeenCalledTimes(1);
    expect(result.current.summary).toEqual(REFETCHED_SUMMARY);
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('toasts "Could not save" with the server message and resolves null on failure', async () => {
    const { result, refetched } = await setupWithSummary(useCreateEntity, [
      failing(CREATE_ENTITY, { input }),
    ]);
    let created: unknown;
    await act(async () => {
      created = await result.current.hook.create(input);
    });
    expect(created).toBeNull();
    expect(toast.error).toHaveBeenCalledWith('Could not save: name must not be blank');
    expect(refetched).not.toHaveBeenCalled();
  });
});

describe('useUpdateEntity', () => {
  it('sends the complete input, returns the entity and refetches the summary', async () => {
    const moved = { ...drill, parentId: 'house' };
    const movedInput = { ...input, parentId: 'house' };
    const { result, refetched } = await setupWithSummary(useUpdateEntity, [
      {
        request: { query: UPDATE_ENTITY, variables: { id: 'drill', input: movedInput } },
        result: { data: { updateEntity: moved } },
      },
    ]);
    let updated: unknown;
    await act(async () => {
      updated = await result.current.hook.update(drill, movedInput);
    });
    expect(updated).toEqual(moved);
    expect(refetched).toHaveBeenCalledTimes(1);
    expect(result.current.summary).toEqual(REFETCHED_SUMMARY);
  });

  it('toasts "Could not save" and resolves null on failure', async () => {
    const { result } = await setupWithSummary(useUpdateEntity, [
      failing(UPDATE_ENTITY, { id: 'drill', input }),
    ]);
    let updated: unknown;
    await act(async () => {
      updated = await result.current.hook.update(drill, input);
    });
    expect(updated).toBeNull();
    expect(toast.error).toHaveBeenCalledWith('Could not save: name must not be blank');
  });
});

describe('useDeleteEntity', () => {
  it('deletes, resolves true and refetches the summary', async () => {
    const { result, refetched } = await setupWithSummary(useDeleteEntity, [
      {
        request: { query: DELETE_ENTITY, variables: { id: 'drill' } },
        result: { data: { deleteEntity: true } },
      },
    ]);
    let deleted: unknown;
    await act(async () => {
      deleted = await result.current.hook.remove(drill);
    });
    expect(deleted).toBe(true);
    expect(refetched).toHaveBeenCalledTimes(1);
    expect(result.current.summary).toEqual(REFETCHED_SUMMARY);
  });

  it('toasts "Could not delete" with the server message and resolves false on failure', async () => {
    const { result } = await setupWithSummary(useDeleteEntity, [
      {
        request: { query: DELETE_ENTITY, variables: { id: 'garage' } },
        result: { errors: [{ message: 'Garage still contains 2 entities; move them first' }] },
      },
    ]);
    let deleted: unknown;
    await act(async () => {
      deleted = await result.current.hook.remove({ id: 'garage', parentId: null });
    });
    expect(deleted).toBe(false);
    expect(toast.error).toHaveBeenCalledWith(
      'Could not delete: Garage still contains 2 entities; move them first',
    );
  });
});
