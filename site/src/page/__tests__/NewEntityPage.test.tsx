import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { CREATE_ENTITY } from 'hooks/queries';
import { EntityInput } from 'types/entity';
import { entityDetail } from 'test/entityFixtures';
import { locationsMock, lookupMocks, tagsMock, typesMock } from 'test/formFixtures';
import { entityMock, spiedMock, summaryMock } from 'test/pageMocks';
import { renderRoute } from 'test/renderRoute';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const blank = (overrides: Partial<EntityInput>): EntityInput => ({
  name: '',
  description: null,
  entityTypeId: '',
  parentId: null,
  archived: false,
  quantity: 1,
  insured: false,
  serialNumber: null,
  modelNumber: null,
  manufacturer: null,
  notes: null,
  lifetimeWarranty: false,
  warrantyExpires: null,
  warrantyDetails: null,
  purchaseDate: null,
  purchaseFrom: null,
  purchasePriceCents: 0,
  soldDate: null,
  soldTo: null,
  soldPriceCents: 0,
  soldNotes: null,
  tagIds: [],
  ...overrides,
});

/** The form's lookups, answered for the first load and again for the post-create refetch. */
const lookupsTwice = () => [...lookupMocks(), typesMock(), tagsMock(), locationsMock()];

/** Waits for the type, tag and location lookups to land. */
const loaded = async () => {
  await screen.findByRole('option', { name: 'Tool' });
  await screen.findByRole('checkbox', { name: 'Garden' });
  await screen.findByRole('option', { name: /Office/ });
};

describe('NewEntityPage', () => {
  it('creates a location under the route location and opens its page', async () => {
    const user = userEvent.setup();
    const attic = entityDetail({ id: 'attic', name: 'Attic', parentId: 'garage' }, true);
    const input = blank({ name: 'Attic', entityTypeId: 'type-loc', parentId: 'garage' });
    const created = spiedMock(CREATE_ENTITY, { input }, { createEntity: attic });
    renderRoute('/locations/garage/new?type=type-loc', [
      ...lookupsTwice(),
      created.mock,
      entityMock('attic', attic),
      summaryMock(),
    ]);
    expect(await screen.findByRole('heading', { level: 1, name: 'New location' })).toBeVisible();
    await loaded();
    expect(screen.getByLabelText('Type')).toHaveValue('type-loc');
    expect(screen.getByLabelText('Parent location')).toHaveValue('garage');
    await user.type(screen.getByLabelText('Name'), 'Attic');
    await user.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() =>
      expect(screen.getByTestId('current-path')).toHaveTextContent(/^\/locations\/attic$/),
    );
    expect(await screen.findByRole('heading', { level: 1, name: 'Attic' })).toBeInTheDocument();
    expect(created.result).toHaveBeenCalledTimes(1);
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('creates a top-level item from /new and opens the item page', async () => {
    const user = userEvent.setup();
    const saw = entityDetail({ id: 'saw', name: 'Saw' });
    const input = blank({ name: 'Saw', entityTypeId: 'type-tool' });
    renderRoute('/new', [
      ...lookupsTwice(),
      spiedMock(CREATE_ENTITY, { input }, { createEntity: saw }).mock,
      entityMock('saw', saw),
      summaryMock(),
    ]);
    expect(await screen.findByRole('heading', { level: 1, name: 'New item' })).toBeVisible();
    await loaded();
    await user.selectOptions(screen.getByLabelText('Type'), 'type-tool');
    await user.type(screen.getByLabelText('Name'), 'Saw');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(screen.getByTestId('current-path')).toHaveTextContent(/^\/items\/saw$/),
    );
    expect(await screen.findByRole('heading', { level: 1, name: 'Saw' })).toBeInTheDocument();
  });

  it('stays on the form when the create fails', async () => {
    const user = userEvent.setup();
    const input = blank({ name: 'Saw', entityTypeId: 'type-tool', parentId: 'garage' });
    renderRoute('/locations/garage/new', [
      ...lookupMocks(),
      {
        request: { query: CREATE_ENTITY, variables: { input } },
        result: { errors: [{ message: 'parent must exist' }] },
      },
    ]);
    await loaded();
    await user.selectOptions(screen.getByLabelText('Type'), 'type-tool');
    await user.type(screen.getByLabelText('Name'), 'Saw');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith('Could not save: parent must exist'),
    );
    expect(screen.getByTestId('current-path')).toHaveTextContent(/^\/locations\/garage\/new$/);
    expect(screen.getByLabelText('Name')).toHaveValue('Saw');
  });

  it('Cancel returns to the location', async () => {
    renderRoute('/locations/garage/new', lookupMocks());
    await loaded();
    expect(screen.getByRole('link', { name: 'Cancel' })).toHaveAttribute(
      'href',
      '/locations/garage',
    );
  });
});
