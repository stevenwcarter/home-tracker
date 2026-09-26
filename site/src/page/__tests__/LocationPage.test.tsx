import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { DELETE_ENTITY } from 'hooks/queries';
import { entityDetail, listItem, locationSummary } from 'test/entityFixtures';
import { typesMock } from 'test/formFixtures';
import { entityMock, spiedMock, summaryMock as summary } from 'test/pageMocks';
import { renderRoute } from 'test/renderRoute';
import { aiSettingsMock } from 'test/aiFixtures';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const summaryMock = summary();

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

/** Garage with nothing in it, so its Delete is enabled. */
const emptyGarage = { ...garage, childLocations: [], items: [] };

describe('LocationPage', () => {
  it('offers "Add item(s) with AI" when a key is set', async () => {
    renderRoute('/locations/garage', [
      entityMock('garage', garage),
      summaryMock,
      typesMock(),
      aiSettingsMock(),
    ]);
    expect(await screen.findByRole('button', { name: 'Add item(s) with AI' })).toBeInTheDocument();
  });

  it('offers "Set up AI" when no key is set', async () => {
    renderRoute('/locations/garage', [entityMock('garage', garage), summaryMock, typesMock()]);
    expect(await screen.findByRole('link', { name: 'Set up AI' })).toHaveAttribute(
      'href',
      '/settings/ai',
    );
  });

  it('shows a loading skeleton, then breadcrumbs, heading, child locations and items', async () => {
    renderRoute('/locations/garage', [entityMock('garage', garage), summaryMock, typesMock()]);
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

  it('links "Add item" and "Add location" (with the Location type) to the new-entity form', async () => {
    renderRoute('/locations/garage', [entityMock('garage', garage), summaryMock, typesMock()]);
    expect(await screen.findByRole('link', { name: 'Add item' })).toHaveAttribute(
      'href',
      '/locations/garage/new',
    );
    expect(await screen.findByRole('link', { name: 'Add location' })).toHaveAttribute(
      'href',
      '/locations/garage/new?type=type-loc',
    );
    expect(screen.getByRole('link', { name: 'Edit' })).toHaveAttribute(
      'href',
      '/locations/garage/edit',
    );
  });

  it('asks before deleting; Cancel sends nothing', async () => {
    const user = userEvent.setup();
    const deleted = spiedMock(DELETE_ENTITY, { id: 'garage' }, { deleteEntity: true });
    renderRoute('/locations/garage', [
      entityMock('garage', emptyGarage),
      summaryMock,
      typesMock(),
      deleted.mock,
    ]);
    await user.click(await screen.findByRole('button', { name: 'Delete' }));
    const dialog = screen.getByRole('dialog', { name: 'Delete Garage?' });
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(deleted.result).not.toHaveBeenCalled();
    expect(screen.getByTestId('current-path')).toHaveTextContent(/^\/locations\/garage$/);
  });

  it('deleting a location navigates to its parent location', async () => {
    const user = userEvent.setup();
    const deleted = spiedMock(DELETE_ENTITY, { id: 'garage' }, { deleteEntity: true });
    const house = entityDetail({ id: 'house', name: 'House' }, true);
    renderRoute('/locations/garage', [
      entityMock('garage', emptyGarage),
      summaryMock,
      typesMock(),
      deleted.mock,
      summary(),
      typesMock(),
      entityMock('garage', null),
      entityMock('house', house),
    ]);
    await user.click(await screen.findByRole('button', { name: 'Delete' }));
    const dialog = screen.getByRole('dialog', { name: 'Delete Garage?' });
    await user.click(within(dialog).getByRole('button', { name: 'Delete location' }));
    expect(await screen.findByRole('heading', { level: 1, name: 'House' })).toBeInTheDocument();
    expect(screen.getByTestId('current-path')).toHaveTextContent(/^\/locations\/house$/);
    expect(deleted.result).toHaveBeenCalledTimes(1);
    expect(toast.error).not.toHaveBeenCalled();

    // The parent reuses the same page instance: its delete state must start fresh.
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Delete' }));
    const parentDialog = screen.getByRole('dialog', { name: 'Delete House?' });
    expect(within(parentDialog).getByRole('button', { name: 'Delete location' })).toBeEnabled();
  });

  it('stays on the page when the delete is refused', async () => {
    const user = userEvent.setup();
    renderRoute('/locations/garage', [
      entityMock('garage', emptyGarage),
      summaryMock,
      typesMock(),
      {
        request: { query: DELETE_ENTITY, variables: { id: 'garage' } },
        result: { errors: [{ message: 'Garage still contains 2 entities; move them first' }] },
      },
    ]);
    await user.click(await screen.findByRole('button', { name: 'Delete' }));
    await user.click(screen.getByRole('button', { name: 'Delete location' }));
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(
        'Could not delete: Garage still contains 2 entities; move them first',
      ),
    );
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(screen.getByRole('heading', { level: 1, name: 'Garage' })).toBeInTheDocument();
    expect(screen.getByTestId('current-path')).toHaveTextContent(/^\/locations\/garage$/);
  });

  it('disables Delete while the location holds anything, and says why', async () => {
    renderRoute('/locations/garage', [entityMock('garage', garage), summaryMock, typesMock()]);
    const remove = await screen.findByRole('button', { name: 'Delete' });
    expect(remove).toBeDisabled();
    expect(remove).toHaveAttribute('title', 'Holds 1 location and 1 item; move them first');
  });

  it('enables Delete on an empty location', async () => {
    renderRoute('/locations/garage', [entityMock('garage', emptyGarage), summaryMock, typesMock()]);
    const remove = await screen.findByRole('button', { name: 'Delete' });
    expect(remove).toBeEnabled();
    expect(remove).not.toHaveAttribute('title');
  });

  it('says so when the location has no children and no items', async () => {
    const empty = entityDetail({ id: 'attic', name: 'Attic' }, true);
    renderRoute('/locations/attic', [entityMock('attic', empty), summaryMock, typesMock()]);
    expect(await screen.findByText('Nothing stored here yet.')).toBeInTheDocument();
  });

  it('redirects to the item page when the entity is an item', async () => {
    const drill = entityDetail({ id: 'drill', name: 'Drill' });
    renderRoute('/locations/drill', [entityMock('drill', drill), summaryMock, typesMock()]);
    expect(await screen.findByRole('heading', { level: 1, name: 'Drill' })).toBeInTheDocument();
    expect(screen.getByTestId('current-path')).toHaveTextContent(/^\/items\/drill$/);
  });

  it('shows Not found when the location does not exist', async () => {
    renderRoute('/locations/missing', [entityMock('missing', null), summaryMock, typesMock()]);
    expect(await screen.findByRole('heading', { level: 1, name: 'Not found' })).toBeInTheDocument();
    expect(screen.getByText("That location doesn't exist.")).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Back to home' })).toHaveAttribute('href', '/');
  });
});

describe('LocationPage photos', () => {
  it('shows the gallery and the uploader', async () => {
    const shelfPhoto = {
      __typename: 'Attachment',
      id: 'lp1',
      kind: 'PHOTO',
      primary: true,
      title: 'Garage door',
      mimeType: 'image/jpeg',
      url: '/attachments/lp1?v=abc',
      thumbnailUrl: '/attachments/lp1/thumb/500?v=abc',
    };
    renderRoute('/locations/garage', [
      entityMock('garage', { ...garage, attachments: [shelfPhoto], primaryPhoto: shelfPhoto }),
      summaryMock,
      typesMock(),
    ]);
    const photos = await screen.findByRole('region', { name: 'Photos' });
    expect(within(photos).getByRole('img', { name: 'Garage door' })).toHaveAttribute(
      'src',
      '/attachments/lp1/thumb/300?v=abc',
    );
    expect(within(photos).getByText('Primary')).toBeInTheDocument();
    expect(within(photos).getByRole('button', { name: 'Add photos' })).toBeInTheDocument();
    expect(within(photos).getByRole('group', { name: 'Photo upload' })).toBeInTheDocument();
  });

  it('offers the uploader on a location with no photos', async () => {
    renderRoute('/locations/garage', [entityMock('garage', garage), summaryMock, typesMock()]);
    const photos = await screen.findByRole('region', { name: 'Photos' });
    expect(within(photos).getByText('No photos yet.')).toBeInTheDocument();
    expect(within(photos).getByRole('button', { name: 'Add photos' })).toBeInTheDocument();
  });
});
