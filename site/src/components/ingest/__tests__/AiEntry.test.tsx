import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { AiEntry } from '../AiEntry';
import { GET_OPEN_INGEST_BATCHES } from 'hooks/queries';
import { AI_SETTINGS, aiSettingsMock } from 'test/aiFixtures';
import { CurrentPath } from 'test/CurrentPath';
import { ingestBatch, ingestItem } from 'test/ingestFixtures';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));

const mutations = vi.hoisted(() => ({ createBatch: vi.fn() }));
vi.mock('hooks/useIngestMutations', () => ({
  useIngestMutations: () => ({ ...mutations, loading: false }),
}));

beforeEach(() => {
  vi.clearAllMocks();
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(new Date('2026-09-26T12:00:00Z'));
});

afterEach(() => vi.useRealTimers());

const openMock = (batches: object[]): MockedResponse => ({
  request: { query: GET_OPEN_INGEST_BATCHES, variables: { parentId: 'garage' } },
  result: { data: { openIngestBatches: batches } },
});

const renderEntry = (mocks: MockedResponse[]) =>
  render(
    <MockedProvider mocks={mocks}>
      <MemoryRouter initialEntries={['/locations/garage']}>
        <CurrentPath />
        <Routes>
          <Route path="*" element={<AiEntry parentId="garage" />} />
        </Routes>
      </MemoryRouter>
    </MockedProvider>,
  );

describe('AiEntry', () => {
  it('with a key, offers "Add item(s) with AI", which starts a batch here and opens it', async () => {
    mutations.createBatch.mockResolvedValue(ingestBatch({ id: 'b9' }));
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    renderEntry([aiSettingsMock(), openMock([])]);
    await user.click(await screen.findByRole('button', { name: 'Add item(s) with AI' }));
    expect(mutations.createBatch).toHaveBeenCalledWith('garage');
    expect(await screen.findByTestId('current-path')).toHaveTextContent('/ingest/b9');
    expect(screen.queryByRole('link', { name: 'Set up AI' })).not.toBeInTheDocument();
  });

  it('a double tap starts one batch', async () => {
    let resolve: (batch: unknown) => void = () => {};
    mutations.createBatch.mockReturnValue(new Promise((done) => (resolve = done)));
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    renderEntry([aiSettingsMock(), openMock([])]);
    const button = await screen.findByRole('button', { name: 'Add item(s) with AI' });
    await user.dblClick(button);
    expect(mutations.createBatch).toHaveBeenCalledTimes(1);
    await act(async () => resolve(ingestBatch({ id: 'b9' })));
    expect(await screen.findByTestId('current-path')).toHaveTextContent('/ingest/b9');
  });

  it('stays put when the batch could not be started', async () => {
    mutations.createBatch.mockResolvedValue(null);
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    renderEntry([aiSettingsMock(), openMock([])]);
    await user.click(await screen.findByRole('button', { name: 'Add item(s) with AI' }));
    expect(screen.getByTestId('current-path')).toHaveTextContent('/locations/garage');
  });

  it('without a key, shows a muted "Set up AI" link to the AI settings instead', async () => {
    renderEntry([aiSettingsMock({ ...AI_SETTINGS, hasApiKey: false }), openMock([])]);
    const link = await screen.findByRole('link', { name: 'Set up AI' });
    expect(link).toHaveAttribute('href', '/settings/ai');
    expect(link).toHaveClass('text-muted');
    expect(screen.queryByRole('button', { name: 'Add item(s) with AI' })).not.toBeInTheDocument();
  });

  it('lists a "Resume batch" link per unfinished batch with its item count and age', async () => {
    renderEntry([
      aiSettingsMock({ ...AI_SETTINGS, hasApiKey: false }),
      openMock([
        ingestBatch({
          id: 'b1',
          createdAt: '2026-09-26T10:00:00Z',
          items: [ingestItem({ id: 'i1' }), ingestItem({ id: 'i2', position: 3 })],
        }),
        ingestBatch({
          id: 'b2',
          createdAt: '2026-09-26T11:55:00Z',
          items: [ingestItem({ id: 'i3' })],
        }),
      ]),
    ]);
    const list = await screen.findByRole('list', { name: 'Unfinished AI batches' });
    const links = within(list).getAllByRole('link');
    expect(links).toHaveLength(2);
    expect(links[0]).toHaveAttribute('href', '/ingest/b1');
    expect(links[0]).toHaveTextContent('Resume batch');
    expect(links[0]).toHaveTextContent('2 items, started 2 hours ago');
    expect(links[1]).toHaveAttribute('href', '/ingest/b2');
    expect(links[1]).toHaveTextContent('1 item, started 5 minutes ago');
  });

  it('has no resume list when nothing is unfinished', async () => {
    renderEntry([aiSettingsMock(), openMock([])]);
    await screen.findByRole('button', { name: 'Add item(s) with AI' });
    expect(screen.queryByRole('list', { name: 'Unfinished AI batches' })).not.toBeInTheDocument();
  });
});
