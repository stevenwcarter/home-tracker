import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { EntityForm } from '../EntityForm';
import { EntityInput } from 'types/entity';
import { entityDetail, locationSummary } from 'test/entityFixtures';
import { lookupMocks, renderWithApollo } from 'test/formFixtures';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));

beforeEach(() => vi.clearAllMocks());

type FormProps = Parameters<typeof EntityForm>[0];

// A partial of the props union (mode plus overrides), completed to a full set below.
const renderForm = (props: Partial<Record<keyof FormProps, unknown>> = {}) => {
  const onSubmit = vi.fn();
  const all = { mode: 'create', onSubmit, submitting: false, ...props } as FormProps;
  renderWithApollo(<EntityForm {...all} />, lookupMocks());
  return { onSubmit };
};

/** Waits for the type, tag and location lookups to land. */
const loaded = async () => {
  await screen.findByRole('option', { name: 'Tool' });
  await screen.findByRole('checkbox', { name: 'Garden' });
  await screen.findByRole('option', { name: /Office/ });
};

const save = () => screen.getByRole('button', { name: 'Save' });

const drill = entityDetail({
  id: 'drill',
  name: 'Drill',
  description: 'Cordless',
  parentId: 'garage',
  parent: locationSummary({ id: 'garage', name: 'Garage', parentId: 'house' }),
  archived: false,
  quantity: 2,
  insured: true,
  serialNumber: 'SN-1',
  modelNumber: 'M-1',
  manufacturer: 'Acme',
  notes: 'Left-handed chuck',
  lifetimeWarranty: false,
  warrantyExpires: '2027-01-02',
  warrantyDetails: 'Two years',
  purchaseDate: '2025-05-06',
  purchaseFrom: 'Hardware store',
  purchasePriceCents: 1250,
  soldDate: '2026-02-03',
  soldTo: 'Neighbour',
  soldPriceCents: 500,
  soldNotes: 'Cash',
  tags: [{ __typename: 'Tag', id: 'garden', name: 'Garden', color: null }] as never,
});

// Written out rather than derived with `toEntityInput`, so a field that
// helper (or the form) drops shows up here as a failure.
const DRILL_INPUT: EntityInput = {
  name: 'Drill',
  description: 'Cordless',
  entityTypeId: 'type-tool',
  parentId: 'garage',
  archived: false,
  quantity: 2,
  insured: true,
  serialNumber: 'SN-1',
  modelNumber: 'M-1',
  manufacturer: 'Acme',
  notes: 'Left-handed chuck',
  lifetimeWarranty: false,
  warrantyExpires: '2027-01-02',
  warrantyDetails: 'Two years',
  purchaseDate: '2025-05-06',
  purchaseFrom: 'Hardware store',
  purchasePriceCents: 1250,
  soldDate: '2026-02-03',
  soldTo: 'Neighbour',
  soldPriceCents: 500,
  soldNotes: 'Cash',
  tagIds: ['garden'],
};

describe('EntityForm', () => {
  describe('submit gating', () => {
    it('keeps Save disabled until a non-blank name is typed', async () => {
      renderForm({ defaultTypeId: 'type-tool' });
      await loaded();
      expect(save()).toBeDisabled();
      await userEvent.type(screen.getByLabelText('Name'), '   ');
      expect(save()).toBeDisabled();
      await userEvent.type(screen.getByLabelText('Name'), 'Drill');
      expect(save()).toBeEnabled();
    });

    it('disables Save again when the name is cleared in edit mode', async () => {
      renderForm({ mode: 'edit', initial: drill });
      await loaded();
      expect(save()).toBeEnabled();
      await userEvent.clear(screen.getByLabelText('Name'));
      expect(save()).toBeDisabled();
    });

    it('disables Save until a type is chosen', async () => {
      renderForm();
      await loaded();
      await userEvent.type(screen.getByLabelText('Name'), 'Drill');
      expect(save()).toBeDisabled();
      await userEvent.selectOptions(screen.getByLabelText('Type'), 'type-tool');
      expect(save()).toBeEnabled();
    });

    it('disables Save while submitting', async () => {
      renderForm({ mode: 'edit', initial: drill, submitting: true });
      await loaded();
      expect(save()).toBeDisabled();
    });
  });

  describe('money fields', () => {
    it.each(['1,2,3', '-1', '1e3', 'abc'])(
      'shows an inline, linked error for %j and blocks Save',
      async (text) => {
        renderForm({ defaultTypeId: 'type-tool' });
        await loaded();
        await userEvent.type(screen.getByLabelText('Name'), 'Drill');
        const price = screen.getByLabelText('Purchase price');
        await userEvent.type(price, text);
        expect(price).toHaveAttribute('aria-invalid', 'true');
        expect(price).toHaveAccessibleDescription('Enter an amount like 12.50');
        expect(save()).toBeDisabled();
        await userEvent.clear(price);
        expect(price).not.toHaveAttribute('aria-invalid', 'true');
        expect(save()).toBeEnabled();
      },
    );

    it('accepts "12." and ".5" and submits them as cents', async () => {
      const { onSubmit } = renderForm({ defaultTypeId: 'type-tool' });
      await loaded();
      await userEvent.type(screen.getByLabelText('Name'), 'Drill');
      await userEvent.type(screen.getByLabelText('Purchase price'), '12.');
      await userEvent.type(screen.getByLabelText('Sold price'), '.5');
      expect(screen.getByLabelText('Purchase price')).not.toHaveAttribute('aria-invalid', 'true');
      expect(screen.getByLabelText('Sold price')).not.toHaveAttribute('aria-invalid', 'true');
      await userEvent.click(save());
      expect(onSubmit).toHaveBeenCalledWith(
        expect.objectContaining({ purchasePriceCents: 1200, soldPriceCents: 50 }),
      );
    });

    it('shows cents in major units in edit mode', async () => {
      renderForm({ mode: 'edit', initial: drill });
      await loaded();
      expect(screen.getByLabelText('Purchase price')).toHaveValue('12.50');
      expect(screen.getByLabelText('Sold price')).toHaveValue('5.00');
    });
  });

  it('flags a blank or fractional quantity inline and blocks Save', async () => {
    renderForm({ mode: 'edit', initial: drill });
    await loaded();
    const quantity = screen.getByLabelText('Quantity');
    await userEvent.clear(quantity);
    expect(quantity).toHaveAccessibleDescription('Enter a whole number, 0 or more');
    expect(save()).toBeDisabled();
    await userEvent.type(quantity, '3');
    expect(save()).toBeEnabled();
  });

  describe('collapsible sections', () => {
    const toggle = (name: string) => screen.getByRole('button', { name });

    it('collapses Purchase, Warranty and Sold for a location type', async () => {
      renderForm({ defaultTypeId: 'type-loc' });
      await loaded();
      for (const name of ['Purchase', 'Warranty', 'Sold']) {
        expect(toggle(name)).toHaveAttribute('aria-expanded', 'false');
      }
      expect(screen.getByLabelText('Purchase price')).not.toBeVisible();
    });

    it('expands them for an item type, following a change of type', async () => {
      renderForm({ defaultTypeId: 'type-loc' });
      await loaded();
      await userEvent.selectOptions(screen.getByLabelText('Type'), 'type-tool');
      for (const name of ['Purchase', 'Warranty', 'Sold']) {
        expect(toggle(name)).toHaveAttribute('aria-expanded', 'true');
      }
      expect(screen.getByLabelText('Purchase price')).toBeVisible();
    });

    it('opens a collapsed section on click', async () => {
      renderForm({ defaultTypeId: 'type-loc' });
      await loaded();
      await userEvent.click(toggle('Purchase'));
      expect(toggle('Purchase')).toHaveAttribute('aria-expanded', 'true');
      expect(screen.getByLabelText('Purchase price')).toBeVisible();
    });

    it('uses the initial entity type before the type list loads', () => {
      renderForm({ mode: 'edit', initial: entityDetail({ id: 'garage', name: 'Garage' }, true) });
      expect(toggle('Sold')).toHaveAttribute('aria-expanded', 'false');
    });
  });

  describe('edit mode', () => {
    it('prefills every field from initial', async () => {
      renderForm({ mode: 'edit', initial: drill });
      await loaded();
      expect(screen.getByLabelText('Name')).toHaveValue('Drill');
      expect(screen.getByLabelText('Type')).toHaveValue('type-tool');
      expect(screen.getByLabelText('Parent location')).toHaveValue('garage');
      expect(screen.getByLabelText('Quantity')).toHaveValue(2);
      expect(screen.getByLabelText('Description')).toHaveValue('Cordless');
      expect(screen.getByLabelText('Purchase date')).toHaveValue('2025-05-06');
      expect(screen.getByLabelText('Purchased from')).toHaveValue('Hardware store');
      expect(screen.getByLabelText('Lifetime warranty')).not.toBeChecked();
      expect(screen.getByLabelText('Warranty expires')).toHaveValue('2027-01-02');
      expect(screen.getByLabelText('Warranty details')).toHaveValue('Two years');
      expect(screen.getByLabelText('Sold date')).toHaveValue('2026-02-03');
      expect(screen.getByLabelText('Sold to')).toHaveValue('Neighbour');
      expect(screen.getByLabelText('Sold notes')).toHaveValue('Cash');
      expect(screen.getByLabelText('Manufacturer')).toHaveValue('Acme');
      expect(screen.getByLabelText('Model number')).toHaveValue('M-1');
      expect(screen.getByLabelText('Serial number')).toHaveValue('SN-1');
      expect(screen.getByLabelText('Insured')).toBeChecked();
      expect(screen.getByLabelText('Notes')).toHaveValue('Left-handed chuck');
      expect(screen.getByLabelText('Archived')).not.toBeChecked();
      expect(screen.getByRole('checkbox', { name: 'Garden' })).toBeChecked();
    });

    it('submits the complete input with only the edited field changed', async () => {
      const { onSubmit } = renderForm({ mode: 'edit', initial: drill });
      await loaded();
      const name = screen.getByLabelText('Name');
      await userEvent.clear(name);
      await userEvent.type(name, 'Hammer drill');
      await userEvent.click(save());
      expect(onSubmit).toHaveBeenCalledTimes(1);
      expect(onSubmit).toHaveBeenCalledWith({ ...DRILL_INPUT, name: 'Hammer drill' });
    });

    it('sends null for a cleared optional field and the full tag set', async () => {
      const { onSubmit } = renderForm({ mode: 'edit', initial: drill });
      await loaded();
      await userEvent.clear(screen.getByLabelText('Manufacturer'));
      await userEvent.clear(screen.getByLabelText('Purchase date'));
      await userEvent.click(screen.getByRole('checkbox', { name: 'Power tools' }));
      await userEvent.click(save());
      expect(onSubmit).toHaveBeenCalledWith({
        ...DRILL_INPUT,
        manufacturer: null,
        purchaseDate: null,
        tagIds: ['garden', 'power'],
      });
    });

    it('never offers the edited location or its descendants as a parent', async () => {
      const garage = entityDetail({ id: 'garage', name: 'Garage', parentId: 'house' }, true);
      renderForm({ mode: 'edit', initial: garage });
      await loaded();
      const options = within(screen.getByLabelText('Parent location'))
        .getAllByRole('option')
        .map((option) => option.textContent?.trim());
      expect(options).toEqual(['None (top level)', 'House', 'Office']);
    });

    it('links Cancel back to the entity page', async () => {
      renderForm({ mode: 'edit', initial: drill });
      await loaded();
      expect(screen.getByRole('link', { name: 'Cancel' })).toHaveAttribute('href', '/items/drill');
    });
  });

  describe('create mode', () => {
    it('submits a complete input with defaults, the default parent and the trimmed name', async () => {
      const { onSubmit } = renderForm({ defaultTypeId: 'type-tool', defaultParentId: 'garage' });
      await loaded();
      expect(screen.getByLabelText('Parent location')).toHaveValue('garage');
      expect(screen.getByLabelText('Purchase price')).toHaveValue('');
      await userEvent.type(screen.getByLabelText('Name'), '  Drill  ');
      await userEvent.click(save());
      expect(onSubmit).toHaveBeenCalledWith({
        name: 'Drill',
        description: null,
        entityTypeId: 'type-tool',
        parentId: 'garage',
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
      } satisfies EntityInput);
    });

    it('links Cancel back to the default parent, or home', async () => {
      renderForm({ defaultParentId: 'garage' });
      await loaded();
      expect(screen.getByRole('link', { name: 'Cancel' })).toHaveAttribute(
        'href',
        '/locations/garage',
      );
    });

    it('links Cancel home without a default parent', async () => {
      renderForm();
      await loaded();
      expect(screen.getByRole('link', { name: 'Cancel' })).toHaveAttribute('href', '/');
    });
  });

  it('labels every input', async () => {
    renderForm({ mode: 'edit', initial: drill });
    await loaded();
    const form = screen.getByRole('form', { name: 'Edit Drill' });
    for (const control of form.querySelectorAll('input, select, textarea')) {
      expect(control).toHaveAccessibleName();
    }
  });
});
