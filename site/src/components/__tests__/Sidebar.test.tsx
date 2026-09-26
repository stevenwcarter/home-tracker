import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MockedProvider } from '@apollo/client/testing/react';
import { MemoryRouter } from 'react-router-dom';
import { Sidebar, EXPANDED_STORAGE_KEY } from '../Sidebar';
import type { MockedResponse } from '@apollo/client/testing';
import { GET_ENTITY, GET_LOCATIONS } from 'hooks/queries';
import { entityDetail } from 'test/entityFixtures';

const locations = [
  { __typename: 'Entity', id: 'house', name: 'House', parentId: null, archived: false },
  { __typename: 'Entity', id: 'garage', name: 'Garage', parentId: 'house', archived: false },
  { __typename: 'Entity', id: 'shelf', name: 'Shelf', parentId: 'garage', archived: false },
];

const LOCATIONS_MOCK: MockedResponse = {
  request: { query: GET_LOCATIONS },
  result: { data: { locations } },
};

const renderSidebar = (path = '/', mocks: MockedResponse[] = [LOCATIONS_MOCK]) =>
  render(
    <MockedProvider mocks={mocks}>
      <MemoryRouter initialEntries={[path]}>
        <Sidebar drawerOpen={false} onClose={() => {}} />
      </MemoryRouter>
    </MockedProvider>,
  );

const storedExpanded = () => JSON.parse(localStorage.getItem(EXPANDED_STORAGE_KEY) ?? 'null');

beforeEach(() => localStorage.clear());

describe('Sidebar', () => {
  it('starts collapsed with nothing stored', async () => {
    renderSidebar();
    expect(await screen.findByRole('link', { name: 'House' })).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Garage' })).not.toBeInTheDocument();
  });

  it('restores the expanded set from localStorage', async () => {
    localStorage.setItem(EXPANDED_STORAGE_KEY, JSON.stringify(['house']));
    renderSidebar();
    expect(await screen.findByRole('link', { name: 'Garage' })).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Shelf' })).not.toBeInTheDocument();
  });

  it('ignores a corrupt stored value', async () => {
    localStorage.setItem(EXPANDED_STORAGE_KEY, '{not json');
    renderSidebar();
    expect(await screen.findByRole('link', { name: 'House' })).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Garage' })).not.toBeInTheDocument();
  });

  it('auto-expands the ancestors of the current location and marks it current', async () => {
    renderSidebar('/locations/shelf');
    const shelf = await screen.findByRole('link', { name: 'Shelf' });
    expect(shelf).toHaveAttribute('aria-current', 'page');
    expect(screen.getByRole('button', { name: 'Collapse House' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Collapse Garage' })).toBeInTheDocument();
    expect(storedExpanded()).toEqual(expect.arrayContaining(['house', 'garage']));
  });

  it('persists toggles to localStorage', async () => {
    renderSidebar();
    await userEvent.click(await screen.findByRole('button', { name: 'Expand House' }));
    expect(screen.getByRole('link', { name: 'Garage' })).toBeInTheDocument();
    expect(storedExpanded()).toEqual(['house']);
    await userEvent.click(screen.getByRole('button', { name: 'Collapse House' }));
    expect(screen.queryByRole('link', { name: 'Garage' })).not.toBeInTheDocument();
    expect(storedExpanded()).toEqual([]);
  });

  it("highlights an item's parent location on /items/:id", async () => {
    const drill = entityDetail({ id: 'drill', name: 'Drill', parentId: 'shelf' });
    renderSidebar('/items/drill', [
      LOCATIONS_MOCK,
      {
        request: { query: GET_ENTITY, variables: { id: 'drill' } },
        result: { data: { entity: drill } },
      },
    ]);
    const shelf = await screen.findByRole('link', { name: 'Shelf' });
    expect(shelf).toHaveAttribute('aria-current', 'page');
    expect(screen.getByRole('button', { name: 'Collapse Garage' })).toBeInTheDocument();
  });

  it('shows an error line, not the empty state, when locations fail to load', async () => {
    renderSidebar('/', [{ request: { query: GET_LOCATIONS }, error: new Error('boom') }]);
    expect(await screen.findByText('Could not load locations.')).toHaveClass('text-danger');
    expect(screen.queryByText('No locations yet.')).not.toBeInTheDocument();
  });
});
