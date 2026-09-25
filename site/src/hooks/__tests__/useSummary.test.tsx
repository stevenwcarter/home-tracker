import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import React from 'react';
import { useSummary } from '../useSummary';
import { GET_SUMMARY } from '../queries';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

const wrapper =
  (mocks: MockedResponse[]) =>
  ({ children }: { children: React.ReactNode }) => (
    <MockedProvider mocks={mocks}>{children}</MockedProvider>
  );

const summary = {
  totalValueCents: 1234567,
  currency: 'USD',
  totalItems: 42,
  totalLocations: 7,
  totalTags: 5,
};

beforeEach(() => vi.clearAllMocks());

describe('useSummary', () => {
  it('exposes the summary once loaded', async () => {
    const mocks = [{ request: { query: GET_SUMMARY }, result: { data: { summary } } }];
    const { result } = renderHook(() => useSummary(), { wrapper: wrapper(mocks) });
    expect(result.current.loading).toBe(true);
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.summary).toEqual(summary);
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('toasts and returns null on error', async () => {
    const mocks = [{ request: { query: GET_SUMMARY }, error: new Error('boom') }];
    const { result } = renderHook(() => useSummary(), { wrapper: wrapper(mocks) });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.summary).toBeNull();
    expect(toast.error).toHaveBeenCalledWith('Error loading summary');
  });

  it('exposes the error', async () => {
    const mocks = [{ request: { query: GET_SUMMARY }, error: new Error('boom') }];
    const { result } = renderHook(() => useSummary(), { wrapper: wrapper(mocks) });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.error).toBeInstanceOf(Error);
  });
});
