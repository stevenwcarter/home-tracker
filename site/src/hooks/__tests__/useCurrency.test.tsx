import { describe, it, expect, vi } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import React from 'react';
import { useCurrency } from '../useCurrency';
import { useSummary } from '../useSummary';
import { GET_SUMMARY } from '../queries';
import { SUMMARY } from 'test/entityFixtures';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));

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
});
