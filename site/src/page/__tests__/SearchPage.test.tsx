import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen } from '@testing-library/react';
import { GET_SUMMARY, SEARCH } from 'hooks/queries';
import { listItem, SUMMARY } from 'test/entityFixtures';
import { renderRoute } from 'test/renderRoute';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const summaryMock = { request: { query: GET_SUMMARY }, result: { data: { summary: SUMMARY } } };
const searchMock = (query: string, search: unknown[]) => ({
  request: { query: SEARCH, variables: { query } },
  result: { data: { search } },
});

describe('SearchPage', () => {
  it('shows results for q, linking items and locations to their pages', async () => {
    renderRoute('/search?q=drill', [
      searchMock('drill', [
        listItem({ id: 'drill', name: 'Drill' }),
        listItem({ id: 'drawer', name: 'Drill drawer' }, true),
      ]),
      summaryMock,
    ]);
    expect(
      screen.getByRole('heading', { level: 1, name: 'Results for "drill"' }),
    ).toBeInTheDocument();
    expect(await screen.findByRole('link', { name: 'Drill' })).toHaveAttribute(
      'href',
      '/items/drill',
    );
    expect(screen.getByRole('link', { name: 'Drill drawer' })).toHaveAttribute(
      'href',
      '/locations/drawer',
    );
    expect(screen.queryByText('No results')).not.toBeInTheDocument();
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('shows the empty state when nothing matches', async () => {
    renderRoute('/search?q=zzz', [searchMock('zzz', []), summaryMock]);
    expect(await screen.findByText('No results')).toBeInTheDocument();
  });

  it('asks for a search term when q is missing', () => {
    renderRoute('/search', [summaryMock]);
    expect(screen.getByRole('heading', { level: 1, name: 'Search' })).toBeInTheDocument();
    expect(screen.getByText('Type in the search box to find items.')).toBeInTheDocument();
  });
});
