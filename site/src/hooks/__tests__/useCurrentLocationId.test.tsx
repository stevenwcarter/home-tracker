import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { useQuery } from '@apollo/client/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import { MemoryRouter } from 'react-router-dom';
import React from 'react';
import { useCurrentLocationId } from '../useCurrentLocationId';
import { GET_ENTITY } from '../queries';
import { entityDetail } from 'test/entityFixtures';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const render = <T,>(
  path: string,
  mocks: MockedResponse[] = [],
  useHook: () => T = () => null as T,
) =>
  renderHook(() => ({ id: useCurrentLocationId(), extra: useHook() }), {
    wrapper: ({ children }: { children: React.ReactNode }) => (
      <MockedProvider mocks={mocks}>
        <MemoryRouter initialEntries={[path]}>{children}</MemoryRouter>
      </MockedProvider>
    ),
  });

describe('useCurrentLocationId', () => {
  it('is the id on /locations/:id', () => {
    expect(render('/locations/shelf').result.current.id).toBe('shelf');
  });

  it("is the item's parent on /items/:id", async () => {
    const drill = entityDetail({ id: 'drill', name: 'Drill', parentId: 'shelf' });
    const { result } = render('/items/drill', [
      {
        request: { query: GET_ENTITY, variables: { id: 'drill' } },
        result: { data: { entity: drill } },
      },
    ]);
    expect(result.current.id).toBeNull();
    await waitFor(() => expect(result.current.id).toBe('shelf'));
  });

  it('is null for a parentless item', async () => {
    const loose = entityDetail({ id: 'loose', name: 'Loose' });
    const { result } = render(
      '/items/loose',
      [
        {
          request: { query: GET_ENTITY, variables: { id: 'loose' } },
          result: { data: { entity: loose } },
        },
      ],
      () => useQuery(GET_ENTITY, { variables: { id: 'loose' } }).data,
    );
    await waitFor(() => expect(result.current.extra).toBeDefined());
    expect(result.current.id).toBeNull();
  });

  it('is null elsewhere', () => {
    expect(render('/').result.current.id).toBeNull();
    expect(render('/search?q=x').result.current.id).toBeNull();
  });

  // A raw probe on the same query shows when the answer has landed.
  const probe = (id: string) => () => useQuery(GET_ENTITY, { variables: { id } });

  it('is null, without a toast, for an unknown item id', async () => {
    const { result } = render(
      '/items/missing',
      [
        {
          request: { query: GET_ENTITY, variables: { id: 'missing' } },
          result: { data: { entity: null } },
        },
      ],
      probe('missing'),
    );
    await waitFor(() => expect(result.current.extra.data).toEqual({ entity: null }));
    expect(result.current.id).toBeNull();
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('is null, without a toast, when the item query fails', async () => {
    const { result } = render(
      '/items/drill',
      [{ request: { query: GET_ENTITY, variables: { id: 'drill' } }, error: new Error('boom') }],
      probe('drill'),
    );
    await waitFor(() => expect(result.current.extra.error).toBeDefined());
    expect(result.current.id).toBeNull();
    expect(toast.error).not.toHaveBeenCalled();
  });
});
