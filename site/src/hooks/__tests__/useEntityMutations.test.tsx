import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { InMemoryCache } from '@apollo/client';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import React from 'react';
import { useCreateEntity, useDeleteEntity, useUpdateEntity } from '../useEntityMutations';
import { CREATE_ENTITY, DELETE_ENTITY, GET_LOCATIONS, UPDATE_ENTITY } from '../queries';
import { entityDetail, locationSummary } from 'test/entityFixtures';
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

/**
 * Renders `useHook` over an explicit cache seeded with House, Garage, Shelf
 * and Drill, each retained so `cache.gc()` keeps them unless they are evicted
 * (the write evicts the inactive `locations` root field that seeded them). No
 * query is active, so nothing refetches and the cache shows exactly what the
 * mutation evicted.
 */
const renderOverSeededCache = <T,>(useHook: () => T, mocks: MockedResponse[]) => {
  const cache = new InMemoryCache();
  cache.writeQuery({
    query: GET_LOCATIONS,
    data: {
      locations: [
        locationSummary({ id: 'house', name: 'House' }),
        locationSummary({ id: 'garage', name: 'Garage', parentId: 'house' }),
        locationSummary({ id: 'shelf', name: 'Shelf', parentId: 'garage' }),
        locationSummary({ id: 'drill', name: 'Drill', parentId: 'garage' }),
      ],
    },
  });
  for (const id of ['house', 'garage', 'shelf', 'drill']) {
    cache.retain(`Entity:${id}`);
  }
  const rendered = renderHook(useHook, {
    wrapper: ({ children }: { children: React.ReactNode }) => (
      <MockedProvider mocks={mocks} cache={cache}>
        {children}
      </MockedProvider>
    ),
  });
  const cached = () => Object.keys(cache.extract());
  return { ...rendered, cached };
};

describe('entity mutation cache eviction', () => {
  it('update on a move evicts the old and the new parent', async () => {
    const movedInput = { ...input, parentId: 'house' };
    const { result, cached } = renderOverSeededCache(useUpdateEntity, [
      {
        request: { query: UPDATE_ENTITY, variables: { id: 'drill', input: movedInput } },
        result: { data: { updateEntity: { ...drill, parentId: 'house' } } },
      },
    ]);
    expect(cached()).toEqual(expect.arrayContaining(['Entity:garage', 'Entity:house']));
    await act(async () => {
      await result.current.update(drill, movedInput);
    });
    expect(cached()).not.toContain('Entity:garage');
    expect(cached()).not.toContain('Entity:house');
    expect(cached()).toContain('Entity:shelf');
  });

  it('update without a move evicts the parent, so a retype re-sorts its sections', async () => {
    const retyped = { ...input, entityTypeId: 'type-loc' };
    const { result, cached } = renderOverSeededCache(useUpdateEntity, [
      {
        request: { query: UPDATE_ENTITY, variables: { id: 'drill', input: retyped } },
        result: { data: { updateEntity: { ...drill, isLocation: true } } },
      },
    ]);
    let updated: unknown;
    await act(async () => {
      updated = await result.current.update(drill, retyped);
    });
    expect(updated).toEqual({ ...drill, isLocation: true });
    expect(cached()).not.toContain('Entity:garage');
    expect(cached()).toEqual(expect.arrayContaining(['Entity:house', 'Entity:shelf']));
  });

  it("create evicts the new entity's parent", async () => {
    const saw = entityDetail({ id: 'saw', name: 'Saw', parentId: 'garage' });
    const sawInput = toEntityInput(saw);
    const { result, cached } = renderOverSeededCache(useCreateEntity, [
      {
        request: { query: CREATE_ENTITY, variables: { input: sawInput } },
        result: { data: { createEntity: saw } },
      },
    ]);
    await act(async () => {
      await result.current.create(sawInput);
    });
    expect(cached()).not.toContain('Entity:garage');
    expect(cached()).toEqual(expect.arrayContaining(['Entity:house', 'Entity:shelf']));
  });

  it('delete evicts the entity and its parent', async () => {
    const { result, cached } = renderOverSeededCache(useDeleteEntity, [
      {
        request: { query: DELETE_ENTITY, variables: { id: 'drill' } },
        result: { data: { deleteEntity: true } },
      },
    ]);
    await act(async () => {
      await result.current.remove(drill);
    });
    expect(cached()).not.toContain('Entity:drill');
    expect(cached()).not.toContain('Entity:garage');
    expect(cached()).toEqual(expect.arrayContaining(['Entity:house', 'Entity:shelf']));
  });
});
