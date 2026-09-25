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
// Reading the stub once to feature-detect it triggers that Experimental
// warning on stderr; swallow just that one warning so `yarn test` output
// stays clean, without hiding any other process warning.
const nodeWebStorageWarning = /localstorage-file/;
const warningListeners = process.listeners('warning') as NodeJS.WarningListener[];
process.removeAllListeners('warning');
process.on('warning', (warning) => {
  if (warning.name === 'ExperimentalWarning' && nodeWebStorageWarning.test(warning.message)) return;
  warningListeners.forEach((listener) => listener(warning));
});

if (typeof globalThis.localStorage === 'undefined') {
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

beforeEach(() => {
  fetchMocker.resetMocks();
  // Default GraphQL answer so App-level tests can render without Apollo mocks.
  fetchMocker.mockIf(/\/graphql$/, async () => ({
    body: JSON.stringify({
      data: {
        summary: {
          totalValueCents: 1234567,
          currency: 'USD',
          totalItems: 42,
          totalLocations: 7,
          totalTags: 5,
        },
      },
    }),
    headers: { 'content-type': 'application/json' },
  }));
});
