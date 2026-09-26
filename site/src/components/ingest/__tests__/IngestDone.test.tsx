import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, within } from '@testing-library/react';
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
  ],
});

const renderDone = () =>
  render(
    <MockedProvider
      mocks={[
        entityMock('drill', entityDetail({ id: 'drill', name: 'Drill' })),
        entityMock('saw', entityDetail({ id: 'saw', name: 'Saw' })),
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
    expect(screen.getByText('Saved 2 items, skipped 1.')).toBeInTheDocument();
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
});
