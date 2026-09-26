import { describe, it, expect, vi } from 'vitest';
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
});
