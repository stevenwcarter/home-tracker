import '@testing-library/jest-dom/vitest';
import createFetchMock from 'vitest-fetch-mock';
import { vi, beforeEach } from 'vitest';

// Node's experimental global Web Storage API (on by default as of Node 26,
// observed via the "localStorage is not available because
// --localstorage-file was not provided" warning) pre-empts jsdom's own
// localStorage: jsdom detects the host already defines `localStorage` and
// skips installing its working implementation, leaving the global resolve
// to `undefined`. Install a minimal in-memory Storage polyfill in that case
// so tests behave the same across Node versions without relying on a
// --no-experimental-webstorage flag that doesn't exist on every Node major.
//
// Reading the stub once to feature-detect it triggers that Experimental
// warning on stderr. Swallow just that one read's warning, and only for
// that read: install a filter immediately before it, then restore the
// original listeners on the next tick. The restore can't happen
// synchronously right after the read — Node emits `warning` via
// `process.nextTick` internally, so the event fires *after* this function
// returns; scheduling the restore with `process.nextTick` too (queued
// after Node's own pending emission) lets the filter catch it before
// stepping aside. Environments where the stub doesn't exist at all (real
// localStorage already works, so there's nothing to swallow) end this tick
// with no process-wiring side effects, and repeated setup-file runs never
// stack wrapper-around-wrapper.
function readGlobalLocalStorage(): typeof globalThis.localStorage {
  const originalListeners = process.listeners('warning') as NodeJS.WarningListener[];
  process.removeAllListeners('warning');
  process.on('warning', (warning) => {
    if (warning.name === 'ExperimentalWarning' && /localstorage-file/.test(warning.message)) return;
    originalListeners.forEach((listener) => listener.call(process, warning));
  });
  const value = globalThis.localStorage;
  process.nextTick(() => {
    process.removeAllListeners('warning');
    originalListeners.forEach((listener) => process.on('warning', listener));
  });
  return value;
}

if (typeof readGlobalLocalStorage() === 'undefined') {
  const createMemoryStorage = (): Storage => {
    const store = new Map<string, string>();
    return {
      get length() {
        return store.size;
      },
      clear: () => store.clear(),
      getItem: (key) => (store.has(key) ? store.get(key)! : null),
      key: (index) => Array.from(store.keys())[index] ?? null,
      removeItem: (key) => void store.delete(key),
      setItem: (key, value) => void store.set(key, String(value)),
    } as Storage;
  };
  globalThis.localStorage = createMemoryStorage();
}

const fetchMocker = createFetchMock(vi);
fetchMocker.enableMocks();

// Default answer for every GraphQL operation, keyed by operation name, so
// App-level tests can render without an Apollo mock of their own. Only
// `summary` carries data worth asserting on; the queries return the empty
// shape a fresh install would show and the deletes succeed. Creates, updates
// and `SetPrimaryPhoto` have no sensible default record, so they (and any
// unknown operation) answer with a GraphQL error naming the operation: a test
// that exercises one must supply its own mock.
const DEFAULT_GRAPHQL_DATA: Record<string, unknown> = {
  GetSummary: {
    summary: {
      totalValueCents: 1234567,
      currency: 'USD',
      totalItems: 42,
      totalLocations: 7,
      totalTags: 5,
    },
  },
  GetLocations: { locations: [] },
  GetRootItems: { rootItems: [] },
  GetEntity: { entity: null },
  Search: { search: [] },
  GetEntityTypes: { entityTypes: [] },
  GetTags: { tags: [] },
  DeleteEntity: { deleteEntity: true },
  DeleteEntityType: { deleteEntityType: true },
  DeleteTag: { deleteTag: true },
  DeleteAttachment: { deleteAttachment: true },
};

beforeEach(() => {
  fetchMocker.resetMocks();
  fetchMocker.mockIf(/\/graphql$/, async (request) => {
    const { operationName } = await request.json();
    const body =
      operationName in DEFAULT_GRAPHQL_DATA
        ? { data: DEFAULT_GRAPHQL_DATA[operationName] }
        : { errors: [{ message: `No default mock for ${operationName}` }] };
    return {
      body: JSON.stringify(body),
      headers: { 'content-type': 'application/json' },
    };
  });
});
