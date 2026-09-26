import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import React from 'react';
import { useSearch } from '../useSearch';
import { SEARCH } from '../queries';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

const wrapper =
  (mocks: MockedResponse[]) =>
  ({ children }: { children: React.ReactNode }) => (
    <MockedProvider mocks={mocks}>{children}</MockedProvider>
  );

const results = [
  {
    __typename: 'Entity',
    id: 'e1',
    name: 'Drill',
    assetId: '000-001',
    quantity: 1,
    purchasePriceCents: 4999,
    archived: false,
    primaryPhoto: null,
    entityType: { __typename: 'EntityType', id: 'et1', name: 'Tool', isLocation: false },
  },
];

beforeEach(() => vi.clearAllMocks());

describe('useSearch', () => {
  it('exposes the results once loaded', async () => {
    const mocks = [
      {
        request: { query: SEARCH, variables: { query: 'drill' } },
        result: { data: { search: results } },
      },
    ];
    const { result } = renderHook(() => useSearch('drill'), { wrapper: wrapper(mocks) });
    expect(result.current.loading).toBe(true);
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.results).toEqual(results);
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('toasts and returns no results on error', async () => {
    const mocks = [
      { request: { query: SEARCH, variables: { query: 'drill' } }, error: new Error('boom') },
    ];
    const { result } = renderHook(() => useSearch('drill'), { wrapper: wrapper(mocks) });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.results).toEqual([]);
    expect(toast.error).toHaveBeenCalledWith('Search failed');
  });

  it('issues no request and returns no results for a blank query', async () => {
    const { result } = renderHook(() => useSearch('   '), { wrapper: wrapper([]) });
    expect(result.current.loading).toBe(false);
    expect(result.current.results).toEqual([]);
    expect(toast.error).not.toHaveBeenCalled();
  });
});
