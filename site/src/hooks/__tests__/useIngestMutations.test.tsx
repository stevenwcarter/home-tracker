import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';
import { InMemoryCache } from '@apollo/client';
import { useQuery } from '@apollo/client/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import React from 'react';
import { useIngestMutations } from '../useIngestMutations';
import {
  ACCEPT_INGEST_ITEM,
  ADD_INGEST_ITEM,
  CREATE_INGEST_BATCH,
  DELETE_INGEST_BATCH,
  GET_INGEST_BATCH,
  GET_LOCATIONS,
  GET_OPEN_INGEST_BATCHES,
  REMOVE_INGEST_ITEM,
  REMOVE_INGEST_PHOTO,
  RETRY_INGEST_ITEM,
  SKIP_INGEST_ITEM,
  SUBMIT_INGEST_BATCH,
} from '../queries';
import { entityDetail, locationSummary } from 'test/entityFixtures';
import { ingestBatch, ingestItem, ingestPhoto } from 'test/ingestFixtures';
import { suggestionToInput } from 'utils/ingest';
import type { IngestBatch, IngestPhotoKindInput } from 'types/ingest';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const ITEM = ingestItem({
  id: 'i1',
  status: 'READY',
  photos: [ingestPhoto({ id: 'p1' }), ingestPhoto({ id: 'p2', suggestedKind: 'RECEIPT' })],
});
const BATCH = ingestBatch({ id: 'b1', parentId: 'garage', status: 'REVIEWING', items: [ITEM] });
const INPUT = {
  ...suggestionToInput(null, { typeId: 'type-tool', parentId: 'shelf', tags: [] }),
  name: 'Edited mouse',
};
const KINDS: IngestPhotoKindInput[] = [
  { photoId: 'p1', kind: 'PHOTO' },
  { photoId: 'p2', kind: 'WARRANTY' },
];
const MOUSE = entityDetail({ id: 'mouse', name: 'Edited mouse', parentId: 'shelf' });

type Hook = ReturnType<typeof useIngestMutations>;

/**
 * Renders the hook over a cache seeded with the locations House > Garage >
 * Shelf (retained, so only an eviction removes them) and, with `watchBatch`,
 * an active `GetIngestBatch` for b1 whose second answer is spied.
 */
const setup = (mocks: MockedResponse[], watchBatch = false) => {
  const cache = new InMemoryCache();
  cache.writeQuery({
    query: GET_LOCATIONS,
    data: {
      locations: [
        locationSummary({ id: 'house', name: 'House' }),
        locationSummary({ id: 'garage', name: 'Garage', parentId: 'house' }),
        locationSummary({ id: 'shelf', name: 'Shelf', parentId: 'garage' }),
      ],
    },
  });
  for (const id of ['house', 'garage', 'shelf']) cache.retain(`Entity:${id}`);
  const refetched = vi.fn(() => ({ data: { ingestBatch: { ...BATCH, status: 'DONE' } } }));
  const batchMocks: MockedResponse[] = watchBatch
    ? [
        {
          request: { query: GET_INGEST_BATCH, variables: { id: 'b1' } },
          result: { data: { ingestBatch: BATCH } },
        },
        { request: { query: GET_INGEST_BATCH, variables: { id: 'b1' } }, result: refetched },
      ]
    : [];
  const useBoth = () => {
    const hook = useIngestMutations();
    const { data } = useQuery<{ ingestBatch: IngestBatch | null }>(GET_INGEST_BATCH, {
      variables: { id: 'b1' },
      skip: !watchBatch,
    });
    return { hook, batch: data?.ingestBatch };
  };
  const rendered = renderHook(useBoth, {
    wrapper: ({ children }: { children: React.ReactNode }) => (
      <MockedProvider mocks={[...batchMocks, ...mocks]} cache={cache}>
        {children}
      </MockedProvider>
    ),
  });
  const cached = () => Object.keys(cache.extract());
  const rootFields = () => Object.keys(cache.extract().ROOT_QUERY ?? {});
  return { ...rendered, cache, cached, rootFields, refetched };
};

describe('useIngestMutations.acceptItem', () => {
  it('sends the edited input and the chosen kinds, and resolves to the new entity', async () => {
    const accept = vi.fn(() => ({ data: { acceptIngestItem: MOUSE } }));
    const { result } = setup([
      {
        request: {
          query: ACCEPT_INGEST_ITEM,
          variables: { id: 'i1', input: INPUT, photoKinds: KINDS },
        },
        result: accept,
      },
    ]);
    let created: unknown;
    await act(async () => {
      created = await result.current.hook.acceptItem('i1', INPUT, KINDS);
    });
    expect(accept).toHaveBeenCalledTimes(1);
    expect(created).toEqual(MOUSE);
    expect(toast.error).not.toHaveBeenCalled();
  });

  it("evicts the chosen parent (not the batch's) and the inactive entity lists", async () => {
    const { result, cached, rootFields } = setup([
      {
        request: {
          query: ACCEPT_INGEST_ITEM,
          variables: { id: 'i1', input: INPUT, photoKinds: KINDS },
        },
        result: { data: { acceptIngestItem: MOUSE } },
      },
    ]);
    expect(cached()).toEqual(expect.arrayContaining(['Entity:shelf', 'Entity:garage']));
    await act(async () => {
      await result.current.hook.acceptItem('i1', INPUT, KINDS);
    });
    expect(cached()).not.toContain('Entity:shelf');
    expect(cached()).toContain('Entity:garage');
    expect(rootFields()).not.toContain('locations');
  });

  it('refetches the open batch', async () => {
    const { result, refetched } = setup(
      [
        {
          request: {
            query: ACCEPT_INGEST_ITEM,
            variables: { id: 'i1', input: INPUT, photoKinds: KINDS },
          },
          result: { data: { acceptIngestItem: MOUSE } },
        },
      ],
      true,
    );
    await waitFor(() => expect(result.current.batch?.status).toBe('REVIEWING'));
    await act(async () => {
      await result.current.hook.acceptItem('i1', INPUT, KINDS);
    });
    expect(refetched).toHaveBeenCalledTimes(1);
    expect(result.current.batch?.status).toBe('DONE');
  });

  it('toasts "Could not save" with the server message and resolves null on failure', async () => {
    const { result } = setup([
      {
        request: {
          query: ACCEPT_INGEST_ITEM,
          variables: { id: 'i1', input: INPUT, photoKinds: KINDS },
        },
        result: { errors: [{ message: 'item is not ready for review' }] },
      },
    ]);
    let created: unknown;
    await act(async () => {
      created = await result.current.hook.acceptItem('i1', INPUT, KINDS);
    });
    expect(created).toBeNull();
    expect(toast.error).toHaveBeenCalledWith('Could not save: item is not ready for review');
  });
});

const EMPTY_ITEM = ingestItem({ id: 'i2', position: 1 });

/** Each call, the request it must send, the answer, and what the call resolves to. */
const CALLS: Array<
  [string, (hook: Hook) => Promise<unknown>, MockedResponse['request'], object, unknown]
> = [
  [
    'createBatch',
    (hook) => hook.createBatch('garage'),
    { query: CREATE_INGEST_BATCH, variables: { parentId: 'garage' } },
    { createIngestBatch: BATCH },
    BATCH,
  ],
  [
    'addItem',
    (hook) => hook.addItem('b1'),
    { query: ADD_INGEST_ITEM, variables: { batchId: 'b1' } },
    { addIngestItem: EMPTY_ITEM },
    EMPTY_ITEM,
  ],
  [
    'removeItem',
    (hook) => hook.removeItem('i2'),
    { query: REMOVE_INGEST_ITEM, variables: { id: 'i2' } },
    { removeIngestItem: true },
    true,
  ],
  [
    'removePhoto',
    (hook) => hook.removePhoto('p1'),
    { query: REMOVE_INGEST_PHOTO, variables: { id: 'p1' } },
    { removeIngestPhoto: true },
    true,
  ],
  [
    'submit',
    (hook) => hook.submit('b1'),
    { query: SUBMIT_INGEST_BATCH, variables: { id: 'b1' } },
    { submitIngestBatch: BATCH },
    BATCH,
  ],
  [
    'retryItem',
    (hook) => hook.retryItem('i1'),
    { query: RETRY_INGEST_ITEM, variables: { id: 'i1' } },
    { retryIngestItem: ITEM },
    ITEM,
  ],
  [
    'skipItem',
    (hook) => hook.skipItem('i1'),
    { query: SKIP_INGEST_ITEM, variables: { id: 'i1' } },
    { skipIngestItem: ITEM },
    ITEM,
  ],
  [
    'deleteBatch',
    (hook) => hook.deleteBatch('b1'),
    { query: DELETE_INGEST_BATCH, variables: { id: 'b1' } },
    { deleteIngestBatch: true },
    true,
  ],
];

describe('useIngestMutations', () => {
  it.each(CALLS)('%s sends its variables and resolves to the answer', async (...args) => {
    const [, call, request, data, expected] = args;
    const { result } = setup([{ request, result: { data } }]);
    let answer: unknown;
    await act(async () => {
      answer = await call(result.current.hook);
    });
    expect(answer).toEqual(expected);
    expect(toast.error).not.toHaveBeenCalled();
  });

  it.each(CALLS)('%s refetches the open batch', async (...args) => {
    const [, call, request, data] = args;
    const { result, refetched } = setup([{ request, result: { data } }], true);
    await waitFor(() => expect(result.current.batch).toBeDefined());
    await act(async () => {
      await call(result.current.hook);
    });
    expect(refetched).toHaveBeenCalledTimes(1);
  });

  it('evicts the cached open-batch lists when none is mounted', async () => {
    const { result, cache, rootFields } = setup([
      {
        request: { query: CREATE_INGEST_BATCH, variables: { parentId: 'garage' } },
        result: { data: { createIngestBatch: BATCH } },
      },
    ]);
    cache.writeQuery({
      query: GET_OPEN_INGEST_BATCHES,
      variables: { parentId: 'garage' },
      data: { openIngestBatches: [] },
    });
    expect(rootFields()).toContain('openIngestBatches({"parentId":"garage"})');
    await act(async () => {
      await result.current.hook.createBatch('garage');
    });
    expect(rootFields().some((field) => field.startsWith('openIngestBatches'))).toBe(false);
  });

  it.each([
    [
      'removePhoto',
      (hook: Hook) => hook.removePhoto('p1'),
      REMOVE_INGEST_PHOTO,
      { id: 'p1' },
      false,
      'Could not remove the photo: batch is processing',
    ],
    [
      'submit',
      (hook: Hook) => hook.submit('b1'),
      SUBMIT_INGEST_BATCH,
      { id: 'b1' },
      null,
      'Could not submit the batch: batch is processing',
    ],
    [
      'deleteBatch',
      (hook: Hook) => hook.deleteBatch('b1'),
      DELETE_INGEST_BATCH,
      { id: 'b1' },
      false,
      'Could not delete: batch is processing',
    ],
  ] as const)(
    '%s toasts the failure and resolves to its fallback',
    async (_name, call, query, variables, fallback, message) => {
      const { result } = setup([
        { request: { query, variables }, result: { errors: [{ message: 'batch is processing' }] } },
      ]);
      let answer: unknown;
      await act(async () => {
        answer = await call(result.current.hook);
      });
      expect(answer).toBe(fallback);
      expect(toast.error).toHaveBeenCalledWith(message);
    },
  );
});
