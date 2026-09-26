import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import React from 'react';
import { useEntityTypes } from '../useEntityTypes';
import { GET_ENTITY_TYPES } from '../queries';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

const wrapper =
  (mocks: MockedResponse[]) =>
  ({ children }: { children: React.ReactNode }) => (
    <MockedProvider mocks={mocks}>{children}</MockedProvider>
  );

const entityTypes = [
  {
    __typename: 'EntityType',
    id: 'loc',
    name: 'Location',
    description: null,
    icon: null,
    isLocation: true,
    entityCount: 3,
  },
];

beforeEach(() => vi.clearAllMocks());

describe('useEntityTypes', () => {
  it('exposes the types once loaded', async () => {
    const mocks = [{ request: { query: GET_ENTITY_TYPES }, result: { data: { entityTypes } } }];
    const { result } = renderHook(() => useEntityTypes(), { wrapper: wrapper(mocks) });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.entityTypes).toEqual(entityTypes);
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('toasts and returns [] on error', async () => {
    const mocks = [{ request: { query: GET_ENTITY_TYPES }, error: new Error('boom') }];
    const { result } = renderHook(() => useEntityTypes(), { wrapper: wrapper(mocks) });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.entityTypes).toEqual([]);
    expect(toast.error).toHaveBeenCalledWith('Error loading types');
  });
});
