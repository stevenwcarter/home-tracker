import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import React from 'react';
import { useEntity } from '../useEntity';
import { GET_ENTITY } from '../queries';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

const wrapper =
  (mocks: MockedResponse[]) =>
  ({ children }: { children: React.ReactNode }) => (
    <MockedProvider mocks={mocks}>{children}</MockedProvider>
  );

const entity = {
  __typename: 'Entity',
  id: 'e1',
  name: 'Drill',
  description: null,
  entityType: { __typename: 'EntityType', id: 'et1', name: 'Tool', isLocation: false },
  isLocation: false,
  parent: { __typename: 'Entity', id: 'loc1', name: 'Garage', parentId: null, archived: false },
  parentId: 'loc1',
  ancestors: [
    { __typename: 'Entity', id: 'loc1', name: 'Garage', parentId: null, archived: false },
  ],
  childLocations: [],
  items: [],
  archived: false,
  assetId: '000-001',
  quantity: 1,
  insured: false,
  serialNumber: null,
  modelNumber: null,
  manufacturer: null,
  notes: null,
  lifetimeWarranty: false,
  warrantyExpires: null,
  warrantyDetails: null,
  purchaseDate: null,
  purchaseFrom: null,
  purchasePriceCents: 4999,
  soldDate: null,
  soldTo: null,
  soldPriceCents: 0,
  soldNotes: null,
  tags: [],
  attachments: [],
  primaryPhoto: null,
  fields: [],
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
};

beforeEach(() => vi.clearAllMocks());

describe('useEntity', () => {
  it('exposes the entity once loaded', async () => {
    const mocks = [
      { request: { query: GET_ENTITY, variables: { id: 'e1' } }, result: { data: { entity } } },
    ];
    const { result } = renderHook(() => useEntity('e1'), { wrapper: wrapper(mocks) });
    expect(result.current.loading).toBe(true);
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.entity).toEqual(entity);
    expect(result.current.notFound).toBe(false);
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('toasts and returns a null entity on error', async () => {
    const mocks = [
      { request: { query: GET_ENTITY, variables: { id: 'e1' } }, error: new Error('boom') },
    ];
    const { result } = renderHook(() => useEntity('e1'), { wrapper: wrapper(mocks) });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.entity).toBeNull();
    expect(result.current.notFound).toBe(false);
    expect(toast.error).toHaveBeenCalledWith('Error loading item');
  });

  it('sets notFound when the entity does not exist', async () => {
    const mocks = [
      {
        request: { query: GET_ENTITY, variables: { id: 'missing' } },
        result: { data: { entity: null } },
      },
    ];
    const { result } = renderHook(() => useEntity('missing'), { wrapper: wrapper(mocks) });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.entity).toBeNull();
    expect(result.current.notFound).toBe(true);
    expect(toast.error).not.toHaveBeenCalled();
  });
});
