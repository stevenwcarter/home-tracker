import { renderHook, waitFor } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import { expect, vi } from 'vitest';
import React from 'react';
import { GET_SUMMARY } from 'hooks/queries';
import { useSummary } from 'hooks/useSummary';
import { SUMMARY } from './entityFixtures';

/** The summary the refetch returns, distinguishable from the first load. */
export const REFETCHED_SUMMARY = { ...SUMMARY, totalItems: SUMMARY.totalItems + 1 };

/**
 * A first `GetSummary` answer plus a second one for the refetch. The second's
 * `result` is a spy, so a test can assert the mutation actually refetched it.
 */
export const summaryMocks = () => {
  const refetched = vi.fn(() => ({ data: { summary: REFETCHED_SUMMARY } }));
  const mocks: MockedResponse[] = [
    { request: { query: GET_SUMMARY }, result: { data: { summary: SUMMARY } } },
    { request: { query: GET_SUMMARY }, result: refetched },
  ];
  return { mocks, refetched };
};

/**
 * Renders `useHook` next to an active `useSummary` (so `GetSummary` is a live
 * query the mutation can refetch) inside a `MockedProvider` with `mocks`.
 */
export const renderWithSummary = <T,>(useHook: () => T, mocks: MockedResponse[]) =>
  renderHook(() => ({ hook: useHook(), summary: useSummary().summary }), {
    wrapper: ({ children }: { children: React.ReactNode }) => (
      <MockedProvider mocks={mocks}>{children}</MockedProvider>
    ),
  });

/**
 * `renderWithSummary` with a first-load `GetSummary` and a spied refetch
 * prepended to `extra`; resolves once the first summary has loaded.
 */
export const setupWithSummary = async <T,>(useHook: () => T, extra: MockedResponse[]) => {
  const { mocks, refetched } = summaryMocks();
  const rendered = renderWithSummary(useHook, [...mocks, ...extra]);
  await waitFor(() => expect(rendered.result.current.summary).toEqual(SUMMARY));
  return { ...rendered, refetched };
};
