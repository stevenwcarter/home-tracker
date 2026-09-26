import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import { GET_LOCATIONS, GET_ROOT_ITEMS, GET_SUMMARY } from 'hooks/queries';
import { listItem, locationSummary, SUMMARY } from 'test/entityFixtures';
import { renderRoute } from 'test/renderRoute';
import { entityTypeWire, typesMock } from 'test/formFixtures';
import { ITEM_TYPE_ID, LOCATION_TYPE_ID } from 'types/builtIns';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));

beforeEach(() => vi.clearAllMocks());

const summaryMock = { request: { query: GET_SUMMARY }, result: { data: { summary: SUMMARY } } };
const locationsMock = {
  request: { query: GET_LOCATIONS },
  result: {
    data: {
      locations: [
        locationSummary({ id: 'house', name: 'House' }),
        locationSummary({ id: 'garage', name: 'Garage', parentId: 'house' }),
        locationSummary({ id: 'shed', name: 'Shed' }),
      ],
    },
  },
};
const rootItemsMock = {
  request: { query: GET_ROOT_ITEMS },
  result: { data: { rootItems: [listItem({ id: 'ladder', name: 'Ladder' })] } },
};

describe('HomePage', () => {
  it('renders the four statistics', async () => {
    renderRoute('/', [summaryMock, locationsMock, rootItemsMock, typesMock()]);
    expect(await screen.findByText('$12,345.67')).toBeInTheDocument();
    expect(screen.getByText('42')).toBeInTheDocument();
    expect(screen.getByText('7')).toBeInTheDocument();
    expect(screen.getByText('5')).toBeInTheDocument();
    for (const label of ['Total Value', 'Total Items', 'Total Locations', 'Total Tags']) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
  });

  it('shows the root locations as cards', async () => {
    renderRoute('/', [summaryMock, locationsMock, rootItemsMock, typesMock()]);
    const locations = screen.getByRole('region', { name: 'Locations' });
    expect(await within(locations).findByRole('link', { name: 'House' })).toHaveAttribute(
      'href',
      '/locations/house',
    );
    expect(within(locations).getByRole('link', { name: 'Shed' })).toBeInTheDocument();
    expect(within(locations).queryByRole('link', { name: 'Garage' })).not.toBeInTheDocument();
  });

  it('shows the items without a location', async () => {
    renderRoute('/', [summaryMock, locationsMock, rootItemsMock, typesMock()]);
    const rootItems = screen.getByRole('region', { name: 'Items without a location' });
    expect(await within(rootItems).findByRole('link', { name: 'Ladder' })).toHaveAttribute(
      'href',
      '/items/ladder',
    );
  });

  it('shows an error state instead of endless loading skeletons when the query fails', async () => {
    renderRoute('/', [
      { request: { query: GET_SUMMARY }, error: new Error('boom') },
      locationsMock,
      rootItemsMock,
      typesMock(),
    ]);

    expect(await screen.findByText('Could not load statistics.')).toBeInTheDocument();
    expect(screen.getAllByText('unavailable')).toHaveLength(4);
    expect(screen.queryByLabelText(/ loading$/)).not.toBeInTheDocument();
  });

  it('links Add location (with the Location type) and Add item to the top-level new-entity form', async () => {
    renderRoute('/', [summaryMock, locationsMock, rootItemsMock, typesMock()]);
    const locations = screen.getByRole('region', { name: 'Locations' });
    await waitFor(() =>
      expect(within(locations).getByRole('link', { name: 'Add location' })).toHaveAttribute(
        'href',
        '/new?type=type-loc',
      ),
    );
    const rootItems = screen.getByRole('region', { name: 'Items without a location' });
    expect(within(rootItems).getByRole('link', { name: 'Add item' })).toHaveAttribute(
      'href',
      '/new',
    );
  });

  it('prefers the built-in Location type for Add location', async () => {
    renderRoute('/', [
      summaryMock,
      locationsMock,
      rootItemsMock,
      typesMock([
        entityTypeWire('room', 'Room', true),
        entityTypeWire(LOCATION_TYPE_ID, 'Place', true),
        entityTypeWire(ITEM_TYPE_ID, 'Item', false),
      ]),
    ]);
    const locations = screen.getByRole('region', { name: 'Locations' });
    await waitFor(() =>
      expect(within(locations).getByRole('link', { name: 'Add location' })).toHaveAttribute(
        'href',
        `/new?type=${LOCATION_TYPE_ID}`,
      ),
    );
  });

  it('falls back to the built-in Location id before the types load', () => {
    renderRoute('/', [summaryMock, locationsMock, rootItemsMock, typesMock()]);
    const locations = screen.getByRole('region', { name: 'Locations' });
    expect(within(locations).getByRole('link', { name: 'Add location' })).toHaveAttribute(
      'href',
      `/new?type=${LOCATION_TYPE_ID}`,
    );
  });
});
