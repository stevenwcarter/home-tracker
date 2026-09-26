import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { InMemoryCache } from '@apollo/client';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import { useCreateEntity } from '../useEntityMutations';
import { useSummary } from '../useSummary';
import { useRootItems } from '../useRootItems';
import { CREATE_ENTITY, GET_ROOT_ITEMS, GET_SUMMARY } from '../queries';
import { entityDetail, listItem, SUMMARY } from 'test/entityFixtures';
import { toEntityInput } from 'utils/entityInput';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));

beforeEach(() => vi.clearAllMocks());

const saw = entityDetail({ id: 'saw', name: 'Saw', parentId: null });
const sawInput = toEntityInput(saw);
const createMock: MockedResponse = {
  request: { query: CREATE_ENTITY, variables: { input: sawInput } },
  result: { data: { createEntity: saw } },
};

const SummaryView = () => {
  const { summary } = useSummary();
  return <p>{summary ? `items ${summary.totalItems}` : 'loading summary'}</p>;
};

const RootItemsView = () => {
  const { items, loading } = useRootItems();
  return <p>{loading ? 'loading items' : `root ${items.map((item) => item.name).join(',')}`}</p>;
};

/** A writer that does not mount the list, like the new/edit pages. */
const Creator = () => {
  const { create } = useCreateEntity();
  const [done, setDone] = useState(false);
  return (
    <button type="button" onClick={() => void create(sawInput).then(() => setDone(true))}>
      {done ? 'created' : 'create'}
    </button>
  );
};

/**
 * Mounts `View`, lets it load, unmounts it (as leaving Home does), creates an
 * entity from a component that never used the query, then mounts `View`
 * again. The remount must go to the network, not answer from the stale cache.
 */
const writeWhileUnmounted = async (
  View: () => React.ReactElement,
  mocks: MockedResponse[],
  first: string,
  second: string,
) => {
  const cache = new InMemoryCache();
  const tree = (show: boolean) => (
    <MockedProvider mocks={[...mocks, createMock]} cache={cache}>
      <>
        {show && <View />}
        <Creator />
      </>
    </MockedProvider>
  );
  const { rerender } = render(tree(true));
  expect(await screen.findByText(first)).toBeInTheDocument();
  rerender(tree(false));
  expect(screen.queryByText(first)).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'create' }));
  expect(await screen.findByRole('button', { name: 'created' })).toBeInTheDocument();
  rerender(tree(true));
  expect(await screen.findByText(second)).toBeInTheDocument();
};

describe('a write while a list query is not mounted', () => {
  it('makes the next GetSummary go to the network', async () => {
    await writeWhileUnmounted(
      SummaryView,
      [
        {
          request: { query: GET_SUMMARY },
          result: { data: { summary: { ...SUMMARY, totalItems: 5 } } },
        },
        {
          request: { query: GET_SUMMARY },
          result: { data: { summary: { ...SUMMARY, totalItems: 6 } } },
        },
      ],
      'items 5',
      'items 6',
    );
  });

  it('makes the next GetRootItems go to the network', async () => {
    const drill = listItem({ id: 'drill', name: 'Drill' });
    const sawItem = listItem({ id: 'saw', name: 'Saw' });
    await writeWhileUnmounted(
      RootItemsView,
      [
        { request: { query: GET_ROOT_ITEMS }, result: { data: { rootItems: [drill] } } },
        { request: { query: GET_ROOT_ITEMS }, result: { data: { rootItems: [drill, sawItem] } } },
      ],
      'root Drill',
      'root Drill,Saw',
    );
  });
});
