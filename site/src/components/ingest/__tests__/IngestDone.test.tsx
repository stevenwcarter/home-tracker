import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MockedProvider } from '@apollo/client/testing/react';
import { render } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { IngestDone } from '../IngestDone';
import { entityDetail } from 'test/entityFixtures';
import { CurrentPath } from 'test/CurrentPath';
import { ingestBatch, ingestItem, ingestSuggestion } from 'test/ingestFixtures';
import { entityMock } from 'test/pageMocks';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));

const mutations = vi.hoisted(() => ({ createBatch: vi.fn() }));
vi.mock('hooks/useIngestMutations', () => ({
  useIngestMutations: () => ({ ...mutations, loading: false }),
}));

beforeEach(() => vi.clearAllMocks());

const BATCH = ingestBatch({
  id: 'b1',
  parentId: 'garage',
  status: 'DONE',
  items: [
    ingestItem({
      id: 'i1',
      status: 'ACCEPTED',
      entityId: 'drill',
      suggestion: ingestSuggestion({ name: 'Cordless drill' }),
    }),
    ingestItem({ id: 'i2', status: 'SKIPPED' }),
    ingestItem({ id: 'i3', status: 'ACCEPTED', entityId: 'saw' }),
    ingestItem({ id: 'i4', status: 'ACCEPTED', entityId: 'shelf' }),
    ingestItem({ id: 'i5', status: 'ACCEPTED', entityId: 'gone' }),
  ],
});

const renderDone = () =>
  render(
    <MockedProvider
      mocks={[
        entityMock('drill', entityDetail({ id: 'drill', name: 'Drill' })),
        entityMock('saw', entityDetail({ id: 'saw', name: 'Saw' })),
        entityMock('shelf', entityDetail({ id: 'shelf', name: 'Top shelf' }, true)),
        entityMock('gone', null),
      ]}
    >
      <MemoryRouter initialEntries={['/ingest/b1']}>
        <CurrentPath />
        <Routes>
          <Route
            path="*"
            element={
              <IngestDone batch={BATCH} back={{ name: 'Garage', path: '/locations/garage' }} />
            }
          />
        </Routes>
      </MemoryRouter>
    </MockedProvider>,
  );

describe('IngestDone', () => {
  it('counts saved and skipped items', () => {
    renderDone();
    expect(screen.getByText('Saved 4 items, skipped 1.')).toBeInTheDocument();
  });

  it('links each saved entity by its saved name', async () => {
    renderDone();
    const list = screen.getByRole('list', { name: 'Saved items' });
    expect(await within(list).findByRole('link', { name: 'Drill' })).toHaveAttribute(
      'href',
      '/items/drill',
    );
    expect(await within(list).findByRole('link', { name: 'Saw' })).toHaveAttribute(
      'href',
      '/items/saw',
    );
  });

  it('links a saved location to its location page', async () => {
    renderDone();
    const list = screen.getByRole('list', { name: 'Saved items' });
    expect(await within(list).findByRole('link', { name: 'Top shelf' })).toHaveAttribute(
      'href',
      '/locations/shelf',
    );
  });

  it('names a saved entity that no longer exists without linking it', async () => {
    renderDone();
    const list = screen.getByRole('list', { name: 'Saved items' });
    await within(list).findByRole('link', { name: 'Saw' });
    expect(within(list).getByText('Item 5')).toBeInTheDocument();
    expect(within(list).queryByRole('link', { name: 'Item 5' })).not.toBeInTheDocument();
  });

  it('shows the fallback name, unlinked, until the entity loads', () => {
    renderDone();
    const list = screen.getByRole('list', { name: 'Saved items' });
    expect(within(list).getByText('Cordless drill')).toBeInTheDocument();
    expect(within(list).queryByRole('link')).not.toBeInTheDocument();
  });

  it('has no back link until the parent is known', () => {
    render(
      <MockedProvider mocks={[]}>
        <MemoryRouter>
          <IngestDone batch={BATCH} back={null} />
        </MemoryRouter>
      </MockedProvider>,
    );
    expect(screen.queryByRole('link', { name: /Back to/ })).not.toBeInTheDocument();
  });

  it('links back to the parent', () => {
    renderDone();
    expect(screen.getByRole('link', { name: 'Back to Garage' })).toHaveAttribute(
      'href',
      '/locations/garage',
    );
  });

  it('"Add more items" starts a new batch with the same parent and opens it', async () => {
    mutations.createBatch.mockResolvedValue(ingestBatch({ id: 'b2' }));
    const user = userEvent.setup();
    renderDone();
    await user.click(screen.getByRole('button', { name: 'Add more items' }));
    expect(mutations.createBatch).toHaveBeenCalledWith('garage');
    expect(await screen.findByTestId('current-path')).toHaveTextContent('/ingest/b2');
  });

  it('a double tap on "Add more items" starts one batch', async () => {
    let resolve: (batch: unknown) => void = () => {};
    mutations.createBatch.mockReturnValue(new Promise((done) => (resolve = done)));
    const user = userEvent.setup();
    renderDone();
    await user.dblClick(screen.getByRole('button', { name: 'Add more items' }));
    expect(mutations.createBatch).toHaveBeenCalledTimes(1);
    await act(async () => resolve(ingestBatch({ id: 'b2' })));
    expect(await screen.findByTestId('current-path')).toHaveTextContent('/ingest/b2');
  });
});
