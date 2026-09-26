import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act } from '@testing-library/react';
import { useCreateTag, useDeleteTag, useUpdateTag } from '../useTagMutations';
import { CREATE_TAG, DELETE_TAG, UPDATE_TAG } from '../queries';
import { REFETCHED_SUMMARY, setupWithSummary } from 'test/mutationHarness';
import { TagInput } from 'types/entity';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const input: TagInput = {
  name: 'Garden',
  description: null,
  color: '#00ff00',
  icon: null,
  parentId: 'outdoor',
};
const wire = {
  __typename: 'Tag',
  id: 'garden',
  name: 'Garden',
  description: null,
  color: '#00ff00',
  icon: null,
  parent: { __typename: 'Tag', id: 'outdoor' },
  entityCount: 0,
};
// What the hooks resolve to: the wire tag with `parent` flattened to `parentId`.
const garden = {
  __typename: 'Tag',
  id: 'garden',
  name: 'Garden',
  description: null,
  color: '#00ff00',
  icon: null,
  parentId: 'outdoor',
  entityCount: 0,
};

describe('tag mutations', () => {
  it('create returns the flattened tag and refetches the summary', async () => {
    const { result, refetched } = await setupWithSummary(useCreateTag, [
      {
        request: { query: CREATE_TAG, variables: { input } },
        result: { data: { createTag: wire } },
      },
    ]);
    let created: unknown;
    await act(async () => {
      created = await result.current.hook.create(input);
    });
    expect(created).toEqual(garden);
    expect(refetched).toHaveBeenCalledTimes(1);
    expect(result.current.summary).toEqual(REFETCHED_SUMMARY);
  });

  it('create toasts "Could not save" on failure', async () => {
    const { result } = await setupWithSummary(useCreateTag, [
      {
        request: { query: CREATE_TAG, variables: { input } },
        result: { errors: [{ message: 'parent tag not found' }] },
      },
    ]);
    let created: unknown;
    await act(async () => {
      created = await result.current.hook.create(input);
    });
    expect(created).toBeNull();
    expect(toast.error).toHaveBeenCalledWith('Could not save: parent tag not found');
  });

  it('update returns the tag and refetches the summary', async () => {
    const topLevel = { ...input, parentId: null };
    const { result, refetched } = await setupWithSummary(useUpdateTag, [
      {
        request: { query: UPDATE_TAG, variables: { id: 'garden', input: topLevel } },
        result: { data: { updateTag: { ...wire, parent: null } } },
      },
    ]);
    let updated: unknown;
    await act(async () => {
      updated = await result.current.hook.update('garden', topLevel);
    });
    expect(updated).toEqual({ ...garden, parentId: null });
    expect(refetched).toHaveBeenCalledTimes(1);
  });

  it('delete resolves true and refetches the summary; failure toasts "Could not delete"', async () => {
    const { result, refetched } = await setupWithSummary(useDeleteTag, [
      {
        request: { query: DELETE_TAG, variables: { id: 'garden' } },
        result: { data: { deleteTag: true } },
      },
      {
        request: { query: DELETE_TAG, variables: { id: 'nope' } },
        result: { errors: [{ message: 'tag not found' }] },
      },
    ]);
    let deleted: unknown;
    await act(async () => {
      deleted = await result.current.hook.remove('garden');
    });
    expect(deleted).toBe(true);
    expect(refetched).toHaveBeenCalledTimes(1);
    await act(async () => {
      deleted = await result.current.hook.remove('nope');
    });
    expect(deleted).toBe(false);
    expect(toast.error).toHaveBeenCalledWith('Could not delete: tag not found');
  });
});
