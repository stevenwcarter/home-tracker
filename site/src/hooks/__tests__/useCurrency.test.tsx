import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { useQuery } from '@apollo/client/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import React from 'react';
import { useCurrency } from '../useCurrency';
import { useSummary } from '../useSummary';
import { GET_SUMMARY } from '../queries';
import { SUMMARY } from 'test/entityFixtures';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const wrapper =
  (mocks: MockedResponse[]) =>
  ({ children }: { children: React.ReactNode }) => (
    <MockedProvider mocks={mocks}>{children}</MockedProvider>
  );

describe('useCurrency', () => {
  it('is the currency from the summary once loaded', async () => {
    const mocks = [
      {
        request: { query: GET_SUMMARY },
        result: { data: { summary: { ...SUMMARY, currency: 'EUR' } } },
      },
    ];
    const { result } = renderHook(() => useCurrency(), { wrapper: wrapper(mocks) });
    expect(result.current).toBe('USD');
    await waitFor(() => expect(result.current).toBe('EUR'));
  });

  it('falls back to USD when the summary fails to load', async () => {
    const mocks = [{ request: { query: GET_SUMMARY }, error: new Error('boom') }];
    const { result } = renderHook(() => ({ currency: useCurrency(), summary: useSummary() }), {
      wrapper: wrapper(mocks),
    });
    await waitFor(() => expect(result.current.summary.error).toBeDefined());
    expect(result.current.currency).toBe('USD');
  });

  it('never toasts, even when the summary fails to load', async () => {
    const mocks = [{ request: { query: GET_SUMMARY }, error: new Error('boom') }];
    // A raw (non-toasting) probe on the same query shows when the failure has landed.
    const { result } = renderHook(
      () => ({ currency: useCurrency(), probe: useQuery(GET_SUMMARY) }),
      { wrapper: wrapper(mocks) },
    );
    await waitFor(() => expect(result.current.probe.error).toBeDefined());
    expect(result.current.currency).toBe('USD');
    expect(toast.error).not.toHaveBeenCalled();
  });
});
