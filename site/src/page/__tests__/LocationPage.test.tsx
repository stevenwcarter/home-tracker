import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, within } from '@testing-library/react';
import { GET_ENTITY, GET_SUMMARY } from 'hooks/queries';
import { entityDetail, listItem, locationSummary, SUMMARY } from 'test/entityFixtures';
import { renderRoute } from 'test/renderRoute';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const summaryMock = { request: { query: GET_SUMMARY }, result: { data: { summary: SUMMARY } } };
const entityMock = (id: string, entity: unknown) => ({
  request: { query: GET_ENTITY, variables: { id } },
  result: { data: { entity } },
});

const garage = entityDetail(
  {
    id: 'garage',
    name: 'Garage',
    description: 'The detached one',
    parentId: 'house',
    parent: locationSummary({ id: 'house', name: 'House' }),
    ancestors: [locationSummary({ id: 'house', name: 'House' })],
    childLocations: [listItem({ id: 'shelf', name: 'Shelf' }, true)],
    items: [
      listItem({
        id: 'drill',
        name: 'Drill',
        assetId: '000-001',
        quantity: 2,
        purchasePriceCents: 4999,
      }),
    ],
  },
  true,
);

describe('LocationPage', () => {
  it('shows a loading skeleton, then breadcrumbs, heading, child locations and items', async () => {
    renderRoute('/locations/garage', [entityMock('garage', garage), summaryMock]);
    expect(screen.getByLabelText('Loading location')).toBeInTheDocument();

    expect(await screen.findByRole('heading', { level: 1, name: 'Garage' })).toBeInTheDocument();
    expect(screen.getByRole('navigation', { name: 'Breadcrumb' })).toHaveTextContent(
      'Home/House/Garage',
    );
    expect(screen.getByText('The detached one')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Shelf' })).toHaveAttribute('href', '/locations/shelf');

    const items = screen.getByRole('region', { name: 'Items' });
    expect(within(items).getByRole('link', { name: 'Drill' })).toHaveAttribute(
      'href',
      '/items/drill',
    );
    expect(within(items).getByText('Qty 2')).toBeInTheDocument();
    expect(await within(items).findByText('$49.99')).toBeInTheDocument();
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('offers a disabled ghost "Add item (coming soon)" placeholder', async () => {
    renderRoute('/locations/garage', [entityMock('garage', garage), summaryMock]);
    const button = await screen.findByRole('button', { name: 'Add item (coming soon)' });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute('title', 'Coming in phase 4');
    expect(button).toHaveClass('border', 'border-border', 'text-muted', 'cursor-not-allowed');
    expect(button).not.toHaveClass('bg-accent');
  });

  it('says so when the location has no children and no items', async () => {
    const empty = entityDetail({ id: 'attic', name: 'Attic' }, true);
    renderRoute('/locations/attic', [entityMock('attic', empty), summaryMock]);
    expect(await screen.findByText('Nothing stored here yet.')).toBeInTheDocument();
  });

  it('redirects to the item page when the entity is an item', async () => {
    const drill = entityDetail({ id: 'drill', name: 'Drill' });
    renderRoute('/locations/drill', [entityMock('drill', drill), summaryMock]);
    expect(await screen.findByRole('heading', { level: 1, name: 'Drill' })).toBeInTheDocument();
    expect(screen.getByTestId('current-path')).toHaveTextContent(/^\/items\/drill$/);
  });

  it('shows Not found when the location does not exist', async () => {
    renderRoute('/locations/missing', [entityMock('missing', null), summaryMock]);
    expect(await screen.findByRole('heading', { level: 1, name: 'Not found' })).toBeInTheDocument();
    expect(screen.getByText("That location doesn't exist.")).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Back to home' })).toHaveAttribute('href', '/');
  });
});
