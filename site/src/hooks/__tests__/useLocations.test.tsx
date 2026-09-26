import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import React from 'react';
import { useLocations } from '../useLocations';
import { GET_LOCATIONS } from '../queries';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

const wrapper =
  (mocks: MockedResponse[]) =>
  ({ children }: { children: React.ReactNode }) => (
    <MockedProvider mocks={mocks}>{children}</MockedProvider>
  );

const locations = [
  { __typename: 'Entity', id: 'house', name: 'House', parentId: null, archived: false },
  { __typename: 'Entity', id: 'garage', name: 'Garage', parentId: 'house', archived: false },
];

beforeEach(() => vi.clearAllMocks());

describe('useLocations', () => {
  it('exposes the flat locations and the nested tree once loaded', async () => {
    const mocks = [{ request: { query: GET_LOCATIONS }, result: { data: { locations } } }];
    const { result } = renderHook(() => useLocations(), { wrapper: wrapper(mocks) });
    expect(result.current.loading).toBe(true);
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.locations).toEqual(locations);
    expect(result.current.tree).toHaveLength(1);
    expect(result.current.tree[0].location.id).toBe('house');
    expect(result.current.tree[0].children.map((n) => n.location.id)).toEqual(['garage']);
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('toasts and returns empty locations and tree on error', async () => {
    const mocks = [{ request: { query: GET_LOCATIONS }, error: new Error('boom') }];
    const { result } = renderHook(() => useLocations(), { wrapper: wrapper(mocks) });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.locations).toEqual([]);
    expect(result.current.tree).toEqual([]);
    expect(toast.error).toHaveBeenCalledWith('Error loading locations');
  });
});
