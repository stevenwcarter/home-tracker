import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import React from 'react';
import { useIngestBatch } from '../useIngestBatch';
import { GET_INGEST_BATCH } from '../queries';
import { ingestBatch, ingestItem } from 'test/ingestFixtures';
import { FakeEventSource } from 'test/fakeEventSource';
import type { IngestBatch } from 'types/ingest';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => {
  vi.clearAllMocks();
  vi.useFakeTimers();
  FakeEventSource.reset();
  vi.stubGlobal('EventSource', FakeEventSource);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

/** Answers every `GetIngestBatch` for `id` with `batch`, counting the requests. */
const batchMock = (id: string, batch: IngestBatch | null) => {
  const served = vi.fn(() => ({ data: { ingestBatch: batch } }));
  const mock: MockedResponse = {
    request: { query: GET_INGEST_BATCH, variables: { id } },
    result: served,
    delay: 0,
    maxUsageCount: Number.POSITIVE_INFINITY,
  };
  return { mock, served };
};

const render = (initialId: string, mocks: MockedResponse[]) =>
  renderHook(({ id }: { id: string }) => useIngestBatch(id), {
    initialProps: { id: initialId },
    wrapper: ({ children }: { children: React.ReactNode }) => (
      <MockedProvider mocks={mocks}>{children}</MockedProvider>
    ),
  });

/** Lets fake time pass, flushing the promises the timers release. */
const pass = (ms: number) => act(() => vi.advanceTimersByTimeAsync(ms));

const B1 = ingestBatch({ id: 'b1', items: [ingestItem({ id: 'i1' })] });

describe('useIngestBatch', () => {
  it('loads the batch and opens its event stream', async () => {
    const { mock, served } = batchMock('b1', B1);
    const { result } = render('b1', [mock]);
    expect(result.current.loading).toBe(true);
    await pass(10);
    expect(result.current.batch?.id).toBe('b1');
    expect(result.current.loading).toBe(false);
    expect(served).toHaveBeenCalledTimes(1);
    expect(FakeEventSource.instances).toHaveLength(1);
    expect(FakeEventSource.latest.url).toBe('/api/ingest/batches/b1/events');
    expect(FakeEventSource.latest.listenedTo.sort()).toEqual(['batch', 'item', 'open', 'photo']);
  });

  it('refetches 150 ms after open, so a reconnect sees the current state', async () => {
    const { mock, served } = batchMock('b1', B1);
    render('b1', [mock]);
    await pass(10);
    act(() => FakeEventSource.latest.emit('open', ''));
    await pass(149);
    expect(served).toHaveBeenCalledTimes(1);
    await pass(10);
    expect(served).toHaveBeenCalledTimes(2);
  });

  it('refetches once per burst of events, and again for a later event', async () => {
    const { mock, served } = batchMock('b1', B1);
    render('b1', [mock]);
    await pass(10);
    act(() => {
      FakeEventSource.latest.emit('photo');
      FakeEventSource.latest.emit('item');
      FakeEventSource.latest.emit('photo');
    });
    await pass(200);
    expect(served).toHaveBeenCalledTimes(2);

    act(() => FakeEventSource.latest.emit('item'));
    await pass(200);
    expect(served).toHaveBeenCalledTimes(3);

    act(() => FakeEventSource.latest.emit('batch'));
    await pass(200);
    expect(served).toHaveBeenCalledTimes(4);
  });

  it('keeps pushing back the refetch while events keep coming', async () => {
    const { mock, served } = batchMock('b1', B1);
    render('b1', [mock]);
    await pass(10);
    for (let n = 0; n < 3; n += 1) {
      act(() => FakeEventSource.latest.emit('photo'));
      await pass(100);
    }
    expect(served).toHaveBeenCalledTimes(1);
    await pass(100);
    expect(served).toHaveBeenCalledTimes(2);
  });

  it('closes the stream on unmount and drops a pending refetch', async () => {
    const { mock, served } = batchMock('b1', B1);
    const { unmount } = render('b1', [mock]);
    await pass(10);
    const source = FakeEventSource.latest;
    act(() => source.emit('item'));
    unmount();
    expect(source.closed).toBe(true);
    await pass(500);
    expect(served).toHaveBeenCalledTimes(1);
  });

  it('closes the old stream and opens a new one when the id changes', async () => {
    const one = batchMock('b1', B1);
    const two = batchMock('b2', ingestBatch({ id: 'b2' }));
    const { result, rerender } = render('b1', [one.mock, two.mock]);
    await pass(10);
    const first = FakeEventSource.latest;
    rerender({ id: 'b2' });
    await pass(10);
    expect(first.closed).toBe(true);
    expect(FakeEventSource.instances).toHaveLength(2);
    expect(FakeEventSource.latest.url).toBe('/api/ingest/batches/b2/events');
    expect(result.current.batch?.id).toBe('b2');
  });

  it('leaves no stream open for a batch that is already done', async () => {
    const done = batchMock('b1', { ...B1, status: 'DONE' });
    render('b1', [done.mock]);
    await pass(10);
    // A done batch's stream only ends, and the browser would reconnect to it forever.
    expect(FakeEventSource.instances.filter((source) => !source.closed)).toEqual([]);
  });

  it('closes the stream once a refetch shows the batch done', async () => {
    const { mock, served } = batchMock('b1', B1);
    served.mockImplementationOnce(() => ({ data: { ingestBatch: B1 } }));
    served.mockImplementation(() => ({ data: { ingestBatch: { ...B1, status: 'DONE' } } }));
    const { result } = render('b1', [mock]);
    await pass(10);
    const source = FakeEventSource.latest;
    expect(source.closed).toBe(false);
    act(() => source.emit('batch'));
    await pass(200);
    expect(result.current.batch?.status).toBe('DONE');
    expect(source.closed).toBe(true);
    expect(FakeEventSource.instances).toHaveLength(1);
  });

  it('reports an unknown batch as not found', async () => {
    const { mock } = batchMock('gone', null);
    const { result } = render('gone', [mock]);
    await pass(10);
    expect(result.current.batch).toBeNull();
    expect(result.current.notFound).toBe(true);
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('toasts a failed load', async () => {
    const { result } = render('b1', [
      {
        request: { query: GET_INGEST_BATCH, variables: { id: 'b1' } },
        error: new Error('offline'),
        delay: 0,
      },
    ]);
    await pass(10);
    expect(result.current.error).toBeDefined();
    expect(toast.error).toHaveBeenCalledWith('Error loading the batch');
  });
});
