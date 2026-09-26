import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import React from 'react';
import { useRootItems } from '../useRootItems';
import { GET_ROOT_ITEMS } from '../queries';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

const wrapper =
  (mocks: MockedResponse[]) =>
  ({ children }: { children: React.ReactNode }) => (
    <MockedProvider mocks={mocks}>{children}</MockedProvider>
  );

const rootItems = [
  {
    __typename: 'Entity',
    id: 'e1',
    name: 'Loose Cable',
    assetId: null,
    quantity: 1,
    purchasePriceCents: 0,
    archived: false,
    primaryPhoto: null,
    entityType: { __typename: 'EntityType', id: 'et1', name: 'Misc', isLocation: false },
  },
];

beforeEach(() => vi.clearAllMocks());

describe('useRootItems', () => {
  it('exposes the root items once loaded', async () => {
    const mocks = [{ request: { query: GET_ROOT_ITEMS }, result: { data: { rootItems } } }];
    const { result } = renderHook(() => useRootItems(), { wrapper: wrapper(mocks) });
    expect(result.current.loading).toBe(true);
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.items).toEqual(rootItems);
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('toasts and returns no items on error', async () => {
    const mocks = [{ request: { query: GET_ROOT_ITEMS }, error: new Error('boom') }];
    const { result } = renderHook(() => useRootItems(), { wrapper: wrapper(mocks) });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.items).toEqual([]);
    expect(toast.error).toHaveBeenCalledWith('Error loading items');
  });
});
