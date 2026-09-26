import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import React from 'react';
import { useOpenIngestBatches } from '../useOpenIngestBatches';
import { GET_OPEN_INGEST_BATCHES } from '../queries';
import { ingestBatch } from 'test/ingestFixtures';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const render = (parentId: string | null, mocks: MockedResponse[]) =>
  renderHook(() => useOpenIngestBatches(parentId), {
    wrapper: ({ children }: { children: React.ReactNode }) => (
      <MockedProvider mocks={mocks}>{children}</MockedProvider>
    ),
  });

describe('useOpenIngestBatches', () => {
  it("lists the parent's unfinished batches", async () => {
    const batches = [ingestBatch({ id: 'b2' }), ingestBatch({ id: 'b1', status: 'REVIEWING' })];
    const { result } = render('garage', [
      {
        request: { query: GET_OPEN_INGEST_BATCHES, variables: { parentId: 'garage' } },
        result: { data: { openIngestBatches: batches } },
      },
    ]);
    expect(result.current.loading).toBe(true);
    expect(result.current.batches).toEqual([]);
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.batches.map((batch) => batch.id)).toEqual(['b2', 'b1']);
  });

  it('asks for the batches with no parent when given none', async () => {
    const { result } = render(null, [
      {
        request: { query: GET_OPEN_INGEST_BATCHES, variables: { parentId: null } },
        result: { data: { openIngestBatches: [ingestBatch({ id: 'b3', parentId: null })] } },
      },
    ]);
    await waitFor(() => expect(result.current.batches).toHaveLength(1));
  });

  it('toasts a failed load and lists nothing', async () => {
    const { result } = render('garage', [
      {
        request: { query: GET_OPEN_INGEST_BATCHES, variables: { parentId: 'garage' } },
        error: new Error('offline'),
      },
    ]);
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.batches).toEqual([]);
    expect(toast.error).toHaveBeenCalledWith('Error loading unfinished batches');
  });
});
