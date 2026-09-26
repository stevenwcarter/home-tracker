import { describe, it, expect, vi, afterEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';
import { useApolloClient, useQuery } from '@apollo/client/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import React from 'react';
import { InMemoryCache } from '@apollo/client';
import { useRefetchingMutation } from '../useRefetchingMutation';
import { DELETE_ENTITY, GET_ENTITY, GET_LOCATIONS } from '../queries';
import { entityDetail, listItem, locationSummary, SUMMARY } from 'test/entityFixtures';
import { REFETCHED_SUMMARY, renderWithSummary, summaryMocks } from 'test/mutationHarness';
import { EntityDetail, LocationSummary } from 'types/entity';

afterEach(() => vi.restoreAllMocks());

const deleteMock = (id: string): MockedResponse => ({
  request: { query: DELETE_ENTITY, variables: { id } },
  result: { data: { deleteEntity: true } },
});

describe('useRefetchingMutation', () => {
  it('refetches the named active queries before the mutation resolves', async () => {
    const { mocks, refetched } = summaryMocks();
    const { result } = renderWithSummary(
      () => useRefetchingMutation(DELETE_ENTITY, { refetch: ['GetSummary'] }),
      [...mocks, deleteMock('drill')],
    );
    await waitFor(() => expect(result.current.summary).toEqual(SUMMARY));

    await act(async () => {
      await result.current.hook[0]({ id: 'drill' });
    });
    expect(refetched).toHaveBeenCalledTimes(1);
    expect(result.current.summary).toEqual(REFETCHED_SUMMARY);
  });

  it('skips named queries that are not active, without an unknown-query warning', async () => {
    const warn = vi.spyOn(console, 'warn');
    const { mocks, refetched } = summaryMocks();
    const { result } = renderWithSummary(
      () => useRefetchingMutation(DELETE_ENTITY, { refetch: ['GetSummary', 'GetTags'] }),
      [...mocks, deleteMock('drill')],
    );
    await waitFor(() => expect(result.current.summary).toEqual(SUMMARY));
    await act(async () => {
      await result.current.hook[0]({ id: 'drill' });
    });
    expect(refetched).toHaveBeenCalledTimes(1);
    expect(warn.mock.calls.flat().join(' ')).not.toMatch(/Unknown query/);
  });

  it('exposes the error and rejects when the mutation fails', async () => {
    const { result } = renderHook(
      () => useRefetchingMutation(DELETE_ENTITY, { refetch: ['GetSummary'] }),
      {
        wrapper: ({ children }: { children: React.ReactNode }) => (
          <MockedProvider
            mocks={[
              {
                request: { query: DELETE_ENTITY, variables: { id: 'garage' } },
                result: {
                  errors: [{ message: 'Garage still contains 2 entities; move them first' }],
                },
              },
            ]}
          >
            {children}
          </MockedProvider>
        ),
      },
    );
    await act(async () => {
      await expect(result.current[0]({ id: 'garage' })).rejects.toThrow(/still contains 2/);
    });
    await waitFor(() => expect(result.current[1].error?.message).toMatch(/still contains 2/));
  });

  it('evicts the returned entities, so their pages refetch, while refetched lists keep them', async () => {
    const drill = listItem({ id: 'drill', name: 'Drill' });
    const garageBefore = entityDetail({ id: 'garage', name: 'Garage', items: [drill] }, true);
    const garageAfter = entityDetail({ id: 'garage', name: 'Garage', items: [] }, true);
    const locations = [locationSummary({ id: 'garage', name: 'Garage' })];
    const garageRefetch = vi.fn(() => ({ data: { entity: garageAfter } }));
    const mocks: MockedResponse[] = [
      { request: { query: GET_LOCATIONS }, result: { data: { locations } } },
      { request: { query: GET_LOCATIONS }, result: { data: { locations } } },
      {
        request: { query: GET_ENTITY, variables: { id: 'garage' } },
        result: { data: { entity: garageBefore } },
      },
      { request: { query: GET_ENTITY, variables: { id: 'garage' } }, result: garageRefetch },
      deleteMock('drill'),
    ];
    const { result } = renderHook(
      () => ({
        client: useApolloClient(),
        locations: useQuery<{ locations: LocationSummary[] }>(GET_LOCATIONS).data?.locations,
        garage: useQuery<{ entity: EntityDetail }>(GET_ENTITY, { variables: { id: 'garage' } }).data
          ?.entity,
        mutation: useRefetchingMutation<{ deleteEntity: boolean }, { id: string }>(DELETE_ENTITY, {
          refetch: ['GetLocations'],
          evict: (_data, { id }) => [id, 'garage'],
        }),
      }),
      {
        wrapper: ({ children }: { children: React.ReactNode }) => (
          <MockedProvider mocks={mocks}>{children}</MockedProvider>
        ),
      },
    );
    await waitFor(() => expect(result.current.garage?.items).toHaveLength(1));
    await waitFor(() => expect(result.current.locations).toHaveLength(1));

    await act(async () => {
      await result.current.mutation[0]({ id: 'drill' });
    });

    // The evicted parent's page refetched and no longer lists the deleted item.
    await waitFor(() => expect(result.current.garage?.items).toEqual([]));
    expect(garageRefetch).toHaveBeenCalledTimes(1);
    // The sidebar list still holds the (evicted, then refetched) parent.
    expect(result.current.locations?.map((location) => location.id)).toEqual(['garage']);
    // The deleted entity is gone from the normalized cache.
    expect(result.current.client.cache.extract()).not.toHaveProperty('Entity:drill');
  });

  it('evicts before the refetch, so a refetched list leaves no dangling reference', async () => {
    const locations = [
      locationSummary({ id: 'house', name: 'House' }),
      locationSummary({ id: 'garage', name: 'Garage', parentId: 'house' }),
    ];
    const mocks: MockedResponse[] = [
      { request: { query: GET_LOCATIONS }, result: { data: { locations } } },
      { request: { query: GET_LOCATIONS }, result: { data: { locations } } },
      deleteMock('drill'),
    ];
    const { result } = renderHook(
      () => ({
        client: useApolloClient(),
        locations: useQuery<{ locations: LocationSummary[] }>(GET_LOCATIONS).data?.locations,
        mutation: useRefetchingMutation<{ deleteEntity: boolean }, { id: string }>(DELETE_ENTITY, {
          refetch: ['GetLocations'],
          evict: (_data, { id }) => [id, 'garage'],
        }),
      }),
      {
        wrapper: ({ children }: { children: React.ReactNode }) => (
          <MockedProvider mocks={mocks}>{children}</MockedProvider>
        ),
      },
    );
    await waitFor(() => expect(result.current.locations).toHaveLength(2));

    await act(async () => {
      await result.current.mutation[0]({ id: 'drill' });
    });

    // The refetched list rewrote Garage after its eviction. Evicting after the
    // refetch instead would leave `locations` pointing at a missing
    // `Entity:garage`, forcing a second, unrequested `GetLocations` fetch.
    expect(result.current.client.cache.extract()).toHaveProperty('Entity:garage');
    expect(result.current.locations?.map((location) => location.id)).toEqual(['house', 'garage']);
  });

  it("evicts the call's alsoEvict ids alongside the hook's evict ids", async () => {
    const cache = new InMemoryCache();
    cache.writeQuery({
      query: GET_LOCATIONS,
      data: {
        locations: ['house', 'garage', 'shelf', 'drill'].map((id) =>
          locationSummary({ id, name: id }),
        ),
      },
    });
    const { result } = renderHook(
      () =>
        useRefetchingMutation<{ deleteEntity: boolean }, { id: string }>(DELETE_ENTITY, {
          refetch: [],
          evict: (_data, { id }) => [id],
        }),
      {
        wrapper: ({ children }: { children: React.ReactNode }) => (
          <MockedProvider mocks={[deleteMock('drill')]} cache={cache}>
            {children}
          </MockedProvider>
        ),
      },
    );
    await act(async () => {
      await result.current[0]({ id: 'drill' }, ['garage', null, undefined]);
    });
    const cached = Object.keys(cache.extract());
    expect(cached).not.toContain('Entity:drill');
    expect(cached).not.toContain('Entity:garage');
    expect(cached).toEqual(expect.arrayContaining(['Entity:house', 'Entity:shelf']));
  });
});
