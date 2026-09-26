import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import React from 'react';
import { useTags } from '../useTags';
import { GET_TAGS } from '../queries';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

const wrapper =
  (mocks: MockedResponse[]) =>
  ({ children }: { children: React.ReactNode }) => (
    <MockedProvider mocks={mocks}>{children}</MockedProvider>
  );

const tag = (id: string, parent: string | null) => ({
  __typename: 'Tag',
  id,
  name: id,
  description: null,
  color: null,
  icon: null,
  parent: parent && { __typename: 'Tag', id: parent },
  entityCount: 1,
});

beforeEach(() => vi.clearAllMocks());

describe('useTags', () => {
  it('exposes the tags with the parent flattened to parentId', async () => {
    const mocks = [
      {
        request: { query: GET_TAGS },
        result: { data: { tags: [tag('tools', null), tag('power', 'tools')] } },
      },
    ];
    const { result } = renderHook(() => useTags(), { wrapper: wrapper(mocks) });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.tags).toEqual([
      expect.objectContaining({ id: 'tools', parentId: null, entityCount: 1 }),
      expect.objectContaining({ id: 'power', parentId: 'tools' }),
    ]);
    expect(result.current.tags[0]).not.toHaveProperty('parent');
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('toasts and returns [] on error', async () => {
    const mocks = [{ request: { query: GET_TAGS }, error: new Error('boom') }];
    const { result } = renderHook(() => useTags(), { wrapper: wrapper(mocks) });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.tags).toEqual([]);
    expect(toast.error).toHaveBeenCalledWith('Error loading tags');
  });
});
