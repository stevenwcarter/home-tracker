import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import { InMemoryCache } from '@apollo/client';
import React from 'react';
import { useUploadPhoto, UploadResult } from '../useUploadPhoto';
import { useEntity } from '../useEntity';
import { useSummary } from '../useSummary';
import { GET_ENTITY, GET_ROOT_ITEMS } from '../queries';
import { entityDetail, listItem, SUMMARY } from 'test/entityFixtures';
import { REFETCHED_SUMMARY, summaryMocks } from 'test/mutationHarness';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const fetchSpy = () => vi.mocked(globalThis.fetch);

const jpeg = (name: string) => new File(['jpeg bytes'], name, { type: 'image/jpeg' });

const created = (id: string) => ({
  id,
  kind: 'PHOTO',
  primary: false,
  title: `${id}.jpg`,
  mimeType: 'image/jpeg',
  sizeBytes: 10,
  url: `/attachments/${id}?v=abc`,
  thumbnailUrl: `/attachments/${id}/thumb/500?v=abc`,
});

const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });

/** Answers every upload with `respond(n)`, n counting from 0, and records each request's form. */
const answerUploads = (respond: (n: number) => Response | Promise<Response>) => {
  const forms: FormData[] = [];
  const urls: string[] = [];
  fetchSpy().mockImplementation(async (input, init) => {
    urls.push(String(input));
    forms.push(init?.body as FormData);
    return respond(forms.length - 1);
  });
  return { forms, urls };
};

const DRILL = { id: 'drill', parentId: 'garage' };

const render = <T,>(useHook: () => T, mocks: MockedResponse[] = [], cache = new InMemoryCache()) =>
  renderHook(useHook, {
    wrapper: ({ children }: { children: React.ReactNode }) => (
      <MockedProvider mocks={mocks} cache={cache}>
        {children}
      </MockedProvider>
    ),
  });

describe('useUploadPhoto', () => {
  it('posts each file as its own multipart request, one at a time, with primary on the first only', async () => {
    let release: (() => void) | undefined;
    const { forms, urls } = answerUploads(async (n) => {
      if (n === 0) await new Promise<void>((resolve) => (release = resolve));
      return json(201, created(`p${n}`));
    });
    const { result } = render(() => useUploadPhoto(DRILL));
    const files = [jpeg('front.jpg'), jpeg('side.jpg')];

    let pending: Promise<UploadResult[]> | undefined;
    act(() => {
      pending = result.current.upload(files, { primary: true });
    });
    await waitFor(() => expect(forms).toHaveLength(1));
    // The second request waits for the first: the backend's primary rule is order-dependent.
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(forms).toHaveLength(1);
    await act(async () => {
      release?.();
      await pending;
    });

    expect(urls).toEqual(['/api/upload/drill', '/api/upload/drill']);
    expect(fetchSpy().mock.calls[0][1]?.method).toBe('POST');
    expect(forms[0].get('file')).toBe(files[0]);
    expect(forms[0].get('primary')).toBe('true');
    expect(forms[1].get('file')).toBe(files[1]);
    expect(forms[1].get('primary')).toBeNull();
    await expect(pending).resolves.toEqual([
      { file: files[0], ok: true },
      { file: files[1], ok: true },
    ]);
  });

  it('sends no primary field without the option', async () => {
    const { forms } = answerUploads(() => json(201, created('p0')));
    const { result } = render(() => useUploadPhoto(DRILL));
    await act(async () => {
      await result.current.upload([jpeg('front.jpg')]);
    });
    expect(forms[0].get('primary')).toBeNull();
  });

  it.each([
    [413, 'File is larger than 25 MB'],
    [415, 'Only JPEG, PNG, GIF and WebP images are supported'],
    [422, 'That file is not a readable image'],
    [404, 'entity not found'],
  ])('maps a %i to "%s", reports it per file and toasts it', async (status, message) => {
    answerUploads(() => json(status, { error: 'entity not found' }));
    const { result } = render(() => useUploadPhoto(DRILL));
    const file = jpeg('big.jpg');
    let results: UploadResult[] = [];
    await act(async () => {
      results = await result.current.upload([file]);
    });
    expect(results).toEqual([{ file, ok: false, error: message }]);
    expect(result.current.progress).toEqual([{ file, status: 'error', error: message }]);
    expect(toast.error).toHaveBeenCalledWith(`Could not upload big.jpg: ${message}`);
  });

  it('refuses a file over the size cap without posting it, and carries on with the next', async () => {
    // The server's 413 for an oversized body arrives before the body is read,
    // so a browser sees a reset connection instead: the cap is checked here.
    const { forms } = answerUploads(() => json(201, created('p1')));
    const { result } = render(() => useUploadPhoto(DRILL));
    const big = jpeg('huge.jpg');
    Object.defineProperty(big, 'size', { value: 26 * 1024 * 1024 });
    const small = jpeg('small.jpg');
    let results: UploadResult[] = [];
    await act(async () => {
      results = await result.current.upload([big, small], { primary: true });
    });
    expect(results).toEqual([
      { file: big, ok: false, error: 'File is larger than 25 MB' },
      { file: small, ok: true },
    ]);
    expect(forms.map((form) => form.get('file'))).toEqual([small]);
    expect(toast.error).toHaveBeenCalledWith(
      'Could not upload huge.jpg: File is larger than 25 MB',
    );
  });

  it("passes a 403's server message through", async () => {
    answerUploads(() => json(403, { error: 'this instance is read-only' }));
    const { result } = render(() => useUploadPhoto(DRILL));
    const file = jpeg('a.jpg');
    let results: UploadResult[] = [];
    await act(async () => {
      results = await result.current.upload([file]);
    });
    expect(results).toEqual([{ file, ok: false, error: 'this instance is read-only' }]);
    expect(toast.error).toHaveBeenCalledWith('Could not upload a.jpg: this instance is read-only');
  });

  it('falls back to the status when the error body is not JSON', async () => {
    answerUploads(() => new Response('<html>Bad gateway</html>', { status: 502 }));
    const { result } = render(() => useUploadPhoto(DRILL));
    let results: UploadResult[] = [];
    await act(async () => {
      results = await result.current.upload([jpeg('a.jpg')]);
    });
    expect(results[0].error).toBe('Upload failed (HTTP 502)');
  });

  it('reports a network failure without rejecting, and carries on with the next file', async () => {
    answerUploads((n) => {
      if (n === 0) throw new TypeError('Failed to fetch');
      return json(201, created('p1'));
    });
    const { result } = render(() => useUploadPhoto(DRILL));
    const files = [jpeg('a.jpg'), jpeg('b.jpg')];
    let results: UploadResult[] = [];
    await act(async () => {
      results = await result.current.upload(files);
    });
    expect(results).toEqual([
      { file: files[0], ok: false, error: 'Could not reach the server' },
      { file: files[1], ok: true },
    ]);
  });

  it('reports progress per file: queued, uploading, then done or error', async () => {
    const releases: Array<() => void> = [];
    answerUploads(async (n) => {
      await new Promise<void>((resolve) => releases.push(resolve));
      return n === 0 ? json(201, created('p0')) : json(415, { error: 'nope' });
    });
    const { result } = render(() => useUploadPhoto(DRILL));
    const files = [jpeg('a.jpg'), jpeg('b.jpg')];
    expect(result.current.uploading).toBe(false);
    expect(result.current.progress).toEqual([]);

    let pending: Promise<UploadResult[]> | undefined;
    act(() => {
      pending = result.current.upload(files);
    });
    await waitFor(() =>
      expect(result.current.progress).toEqual([
        { file: files[0], status: 'uploading' },
        { file: files[1], status: 'queued' },
      ]),
    );
    expect(result.current.uploading).toBe(true);

    await act(async () => releases[0]());
    await waitFor(() =>
      expect(result.current.progress).toEqual([
        { file: files[0], status: 'done' },
        { file: files[1], status: 'uploading' },
      ]),
    );

    await act(async () => {
      releases[1]();
      await pending;
    });
    expect(result.current.progress).toEqual([
      { file: files[0], status: 'done' },
      {
        file: files[1],
        status: 'error',
        error: 'Only JPEG, PNG, GIF and WebP images are supported',
      },
    ]);
    expect(result.current.uploading).toBe(false);
  });

  it('after a success, refetches active queries and evicts the entity, its parent and inactive lists', async () => {
    answerUploads(() => json(201, created('p0')));
    const cache = new InMemoryCache();
    cache.writeQuery({
      query: GET_ENTITY,
      variables: { id: 'garage' },
      data: { entity: entityDetail({ id: 'garage', name: 'Garage' }, true) },
    });
    cache.writeQuery({
      query: GET_ROOT_ITEMS,
      data: { rootItems: [listItem({ id: 'hammer', name: 'Hammer' })] },
    });
    const { mocks, refetched } = summaryMocks();
    const { result } = render(
      () => ({ hook: useUploadPhoto(DRILL), summary: useSummary().summary }),
      mocks,
      cache,
    );
    await waitFor(() => expect(result.current.summary).toEqual(SUMMARY));

    await act(async () => {
      await result.current.hook.upload([jpeg('front.jpg')]);
    });

    // The active summary was refetched before `upload` resolved.
    expect(refetched).toHaveBeenCalledTimes(1);
    expect(result.current.summary).toEqual(REFETCHED_SUMMARY);
    const cached = cache.extract();
    // The parent's cached page (its item list shows the primary photo) is gone.
    expect(cached).not.toHaveProperty('Entity:garage');
    // `GetRootItems` was inactive, so its root field is evicted, not refetched.
    expect(Object.keys(cached.ROOT_QUERY ?? {})).not.toContain('rootItems');
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('refetches an open entity page exactly once, showing the new photo', async () => {
    answerUploads(() => json(201, created('p0')));
    const before = entityDetail({ id: 'drill', name: 'Drill', parentId: 'garage' });
    const photo = { __typename: 'Attachment', ...created('p0'), primary: true };
    delete (photo as { sizeBytes?: number }).sizeBytes;
    const after = { ...before, attachments: [photo], primaryPhoto: photo };
    const drillRefetch = vi.fn(() => ({ data: { entity: after } }));
    const { result } = render(
      () => ({ hook: useUploadPhoto(DRILL), entity: useEntity('drill').entity }),
      [
        {
          request: { query: GET_ENTITY, variables: { id: 'drill' } },
          result: { data: { entity: before } },
        },
        { request: { query: GET_ENTITY, variables: { id: 'drill' } }, result: drillRefetch },
      ],
    );
    await waitFor(() => expect(result.current.entity?.attachments).toEqual([]));

    await act(async () => {
      await result.current.hook.upload([jpeg('front.jpg')]);
    });

    await waitFor(() => expect(result.current.entity?.attachments).toHaveLength(1));
    expect(drillRefetch).toHaveBeenCalledTimes(1);
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('touches no query when every file fails', async () => {
    answerUploads(() => json(415, { error: 'nope' }));
    const cache = new InMemoryCache();
    cache.writeQuery({
      query: GET_ROOT_ITEMS,
      data: { rootItems: [listItem({ id: 'hammer', name: 'Hammer' })] },
    });
    const { mocks, refetched } = summaryMocks();
    const { result } = render(
      () => ({ hook: useUploadPhoto(DRILL), summary: useSummary().summary }),
      mocks,
      cache,
    );
    await waitFor(() => expect(result.current.summary).toEqual(SUMMARY));
    await act(async () => {
      await result.current.hook.upload([jpeg('a.txt')]);
    });
    expect(refetched).not.toHaveBeenCalled();
    expect(Object.keys(cache.extract().ROOT_QUERY ?? {})).toContain('rootItems');
  });

  it('still resolves with the results when the refetch afterwards fails', async () => {
    answerUploads(() => json(201, created('p0')));
    const { mocks } = summaryMocks();
    // The refetch gets a network error instead of the second summary.
    mocks[1] = { request: mocks[1].request, error: new Error('offline') };
    const { result } = render(
      () => ({ hook: useUploadPhoto(DRILL), summary: useSummary().summary }),
      mocks,
    );
    await waitFor(() => expect(result.current.summary).toEqual(SUMMARY));
    const file = jpeg('front.jpg');
    let results: UploadResult[] = [];
    await act(async () => {
      results = await result.current.hook.upload([file]);
    });
    expect(results).toEqual([{ file, ok: true }]);
    expect(result.current.hook.uploading).toBe(false);
  });
});
