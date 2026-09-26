import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { CREATE_ENTITY_TYPE, DELETE_ENTITY_TYPE, UPDATE_ENTITY_TYPE } from 'hooks/queries';
import { entityTypeWire, typesMock } from 'test/formFixtures';
import { spiedMock } from 'test/pageMocks';
import { renderRoute } from 'test/renderRoute';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const LOCATION_ID = '00000000-0000-7000-8000-000000000001';
const ITEM_ID = '00000000-0000-7000-8000-000000000002';

const TYPES = [
  { ...entityTypeWire(LOCATION_ID, 'Location', true), entityCount: 0 },
  { ...entityTypeWire(ITEM_ID, 'Item', false), entityCount: 0 },
  { ...entityTypeWire('tote', 'Tote', true), entityCount: 3 },
  { ...entityTypeWire('tool', 'Tool', false), entityCount: 1 },
  { ...entityTypeWire('gadget', 'Gadget', false), description: 'Small stuff' },
];

const row = (name: string) => screen.getByRole('row', { name: new RegExp(`^${name}\\b`) });

describe('EntityTypesPage', () => {
  it('lists every type with its kind and entity count', async () => {
    renderRoute('/types', [typesMock(TYPES)]);
    expect(await screen.findByRole('heading', { level: 1, name: 'Types' })).toBeVisible();
    await screen.findByRole('cell', { name: /Tote/ });
    const tote = row('Tote');
    expect(within(tote).getByText('Location')).toBeInTheDocument();
    expect(within(tote).getByText('3')).toBeInTheDocument();
    expect(within(row('Gadget')).getByText('Small stuff')).toBeInTheDocument();
    expect(within(row('Tool')).getByText('Item')).toBeInTheDocument();
  });

  it('disables Delete, with a reason, for types in use and for the built-ins', async () => {
    renderRoute('/types', [typesMock(TYPES)]);
    await screen.findByRole('cell', { name: /Tote/ });
    const toteDelete = within(row('Tote')).getByRole('button', { name: 'Delete Tote' });
    expect(toteDelete).toBeDisabled();
    expect(toteDelete).toHaveAttribute('title', 'In use by 3 entities');
    expect(within(row('Tool')).getByRole('button', { name: 'Delete Tool' })).toHaveAttribute(
      'title',
      'In use by 1 entity',
    );
    for (const name of ['Location', 'Item']) {
      const button = within(row(name)).getByRole('button', { name: `Delete ${name}` });
      expect(button).toBeDisabled();
      expect(button).toHaveAttribute('title', 'Built-in types cannot be deleted');
    }
    expect(within(row('Gadget')).getByRole('button', { name: 'Delete Gadget' })).toBeEnabled();
  });

  it('deletes an unused type after confirming', async () => {
    const user = userEvent.setup();
    const deleted = spiedMock(DELETE_ENTITY_TYPE, { id: 'gadget' }, { deleteEntityType: true });
    renderRoute('/types', [
      typesMock(TYPES),
      deleted.mock,
      typesMock(TYPES.filter((type) => type.id !== 'gadget')),
    ]);
    await screen.findByRole('cell', { name: /Gadget/ });
    await user.click(screen.getByRole('button', { name: 'Delete Gadget' }));
    const dialog = screen.getByRole('dialog', { name: 'Delete Gadget?' });
    await user.click(within(dialog).getByRole('button', { name: 'Delete type' }));
    await waitFor(() =>
      expect(screen.queryByRole('cell', { name: /Gadget/ })).not.toBeInTheDocument(),
    );
    expect(deleted.result).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('creates a type from the inline form', async () => {
    const user = userEvent.setup();
    const bin = entityTypeWire('bin', 'Bin', true);
    const input = { name: 'Bin', description: null, icon: null, isLocation: true };
    const created = spiedMock(CREATE_ENTITY_TYPE, { input }, { createEntityType: bin });
    renderRoute('/types', [typesMock(TYPES), created.mock, typesMock([...TYPES, bin])]);
    const form = await screen.findByRole('form', { name: 'New type' });
    const add = within(form).getByRole('button', { name: 'Add type' });
    expect(add).toBeDisabled();
    await user.type(within(form).getByLabelText('Name'), '  Bin ');
    await user.click(within(form).getByLabelText('Holds other things (location)'));
    await user.click(add);
    expect(await screen.findByRole('cell', { name: /^Bin/ })).toBeInTheDocument();
    expect(created.result).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(within(form).getByLabelText('Name')).toHaveValue(''));
  });

  it('edits a type in place', async () => {
    const user = userEvent.setup();
    const input = { name: 'Gizmo', description: 'Small stuff', icon: null, isLocation: false };
    const renamed = { ...TYPES[4], name: 'Gizmo' };
    const updated = spiedMock(
      UPDATE_ENTITY_TYPE,
      { id: 'gadget', input },
      { updateEntityType: renamed },
    );
    renderRoute('/types', [
      typesMock(TYPES),
      updated.mock,
      typesMock([...TYPES.slice(0, 4), renamed]),
    ]);
    await screen.findByRole('cell', { name: /Gadget/ });
    await user.click(screen.getByRole('button', { name: 'Edit Gadget' }));
    const form = screen.getByRole('form', { name: 'Edit Gadget' });
    const name = within(form).getByLabelText('Name');
    await user.clear(name);
    await user.type(name, 'Gizmo');
    await user.click(within(form).getByRole('button', { name: 'Save' }));
    expect(await screen.findByRole('cell', { name: /^Gizmo/ })).toBeInTheDocument();
    expect(screen.queryByRole('form', { name: 'Edit Gadget' })).not.toBeInTheDocument();
    expect(updated.result).toHaveBeenCalledTimes(1);
  });
});
