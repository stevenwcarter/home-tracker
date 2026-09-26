import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ReactNode } from 'react';
import { render, screen, within } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import { MemoryRouter } from 'react-router-dom';
import userEvent from '@testing-library/user-event';
import { IngestReview } from '../IngestReview';
import {
  ENTITY_TYPES,
  entityTypeWire,
  locationsMock,
  tagsMock,
  typesMock,
} from 'test/formFixtures';
import { ingestBatch, ingestItem, ingestPhoto, ingestSuggestion } from 'test/ingestFixtures';
import { ITEM_TYPE_ID } from 'types/builtIns';
import type { EntityInput } from 'types/entity';
import type { IngestBatch } from 'types/ingest';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));

const mutations = vi.hoisted(() => ({
  acceptItem: vi.fn(),
  skipItem: vi.fn(),
  retryItem: vi.fn(),
}));
vi.mock('hooks/useIngestMutations', () => ({
  useIngestMutations: () => ({ ...mutations, loading: false }),
}));

beforeEach(() => vi.clearAllMocks());

const TYPES = [...ENTITY_TYPES, entityTypeWire(ITEM_TYPE_ID, 'Item', false)];

const DRILL = ingestItem({
  id: 'i2',
  position: 5,
  status: 'READY',
  suggestion: ingestSuggestion({
    name: 'Cordless drill',
    manufacturer: 'Acme',
    modelNumber: 'D-18',
    quantity: 1,
    purchaseDate: '2025-05-06',
    purchasePriceCents: 4999,
    tagNames: ['power TOOLS', 'Unknown'],
    confidence: 'high',
    reasoning: 'The receipt names the model.',
  }),
  photos: [
    ingestPhoto({
      id: 'front',
      status: 'DESCRIBED',
      suggestedKind: 'PHOTO',
      summary: 'A yellow drill on a bench',
      text: null,
    }),
    ingestPhoto({
      id: 'receipt',
      position: 1,
      status: 'DESCRIBED',
      suggestedKind: 'RECEIPT',
      summary: 'A till receipt',
      text: 'ACME D-18  49.99',
    }),
  ],
});

const batch = (items = [ingestItem({ id: 'i1', status: 'ACCEPTED' }), DRILL]): IngestBatch =>
  ingestBatch({ id: 'b1', parentId: 'garage', status: 'REVIEWING', items });

/** Rendered with a wrapper, so `rerender` can hand it a changed batch. */
const renderReview = (value: IngestBatch = batch()) => {
  const mocks = [typesMock(TYPES), tagsMock(), locationsMock()];
  const wrapper = ({ children }: { children: ReactNode }) => (
    <MockedProvider mocks={mocks}>
      <MemoryRouter>{children}</MemoryRouter>
    </MockedProvider>
  );
  return render(<IngestReview batch={value} />, { wrapper });
};

/** The form, once the type, tag and location lookups have landed. */
const form = async () => {
  const found = await screen.findByRole('form', { name: 'New entity' });
  await within(found).findByRole('option', { name: /Garage/ });
  return found;
};

describe('IngestReview', () => {
  it('reviews the first ready item, labelled by its index', async () => {
    renderReview();
    expect(await screen.findByRole('heading', { name: 'Item 2' })).toBeInTheDocument();
    await form();
  });

  it('prefills the form from the suggestion, under Item and the batch parent', async () => {
    renderReview();
    const found = await form();
    expect(within(found).getByLabelText('Name')).toHaveValue('Cordless drill');
    expect(within(found).getByLabelText('Manufacturer')).toHaveValue('Acme');
    expect(within(found).getByLabelText('Model number')).toHaveValue('D-18');
    expect(within(found).getByLabelText('Purchase price')).toHaveValue('49.99');
    expect(within(found).getByLabelText('Type')).toHaveValue(ITEM_TYPE_ID);
    expect(within(found).getByRole('checkbox', { name: 'Power tools' })).toBeChecked();
    expect(within(found).getByRole('checkbox', { name: 'Garden' })).not.toBeChecked();
  });

  it('shows the confidence and reasoning above the form', async () => {
    renderReview();
    expect(await screen.findByText(/Confidence: high/)).toHaveTextContent(
      'The receipt names the model.',
    );
  });

  it('shows each photo with a kind select defaulting to its suggested kind', async () => {
    renderReview();
    const strip = await screen.findByRole('list', { name: 'Photos of Item 2' });
    expect(within(strip).getAllByRole('img')[0]).toHaveAttribute(
      'src',
      '/ingest/photos/front/thumb/300?v=abc',
    );
    expect(within(strip).getByLabelText('Kind of photo 1')).toHaveValue('PHOTO');
    expect(within(strip).getByLabelText('Kind of photo 2')).toHaveValue('RECEIPT');
  });

  it('shows what the AI saw in a collapsible section', async () => {
    renderReview();
    const strip = await screen.findByRole('list', { name: 'Photos of Item 2' });
    const [, receipt] = within(strip).getAllByRole('listitem');
    const details = receipt.querySelector('details');
    expect(details).not.toBeNull();
    expect(details).not.toHaveAttribute('open');
    expect(within(receipt).getByText('What the AI saw')).toBeInTheDocument();
    expect(details).toHaveTextContent('A till receipt');
    expect(details).toHaveTextContent('ACME D-18 49.99');
  });

  it('Save item accepts with the edited input and the chosen kinds', async () => {
    mutations.acceptItem.mockResolvedValue({ id: 'drill' });
    const user = userEvent.setup();
    renderReview();
    const found = await form();
    const name = within(found).getByLabelText('Name');
    await user.clear(name);
    await user.type(name, 'Drill');
    await user.selectOptions(screen.getByLabelText('Kind of photo 2'), 'WARRANTY');
    await user.click(within(found).getByRole('button', { name: 'Save item' }));

    const expected: EntityInput = {
      name: 'Drill',
      description: null,
      entityTypeId: ITEM_TYPE_ID,
      parentId: 'garage',
      archived: false,
      quantity: 1,
      insured: false,
      serialNumber: null,
      modelNumber: 'D-18',
      manufacturer: 'Acme',
      notes: null,
      lifetimeWarranty: false,
      warrantyExpires: null,
      warrantyDetails: null,
      purchaseDate: '2025-05-06',
      purchaseFrom: null,
      purchasePriceCents: 4999,
      soldDate: null,
      soldTo: null,
      soldPriceCents: 0,
      soldNotes: null,
      tagIds: ['power'],
    };
    expect(mutations.acceptItem).toHaveBeenCalledWith('i2', expected, [
      { photoId: 'front', kind: 'PHOTO' },
      { photoId: 'receipt', kind: 'WARRANTY' },
    ]);
  });

  it('Skip skips the item', async () => {
    const user = userEvent.setup();
    renderReview();
    await form();
    await user.click(screen.getByRole('button', { name: 'Skip' }));
    expect(mutations.skipItem).toHaveBeenCalledWith('i2');
    expect(mutations.acceptItem).not.toHaveBeenCalled();
  });

  it('moves to the next reviewable item when the batch changes', async () => {
    const { rerender } = renderReview();
    await form();
    const next = ingestItem({
      id: 'i3',
      status: 'READY',
      suggestion: ingestSuggestion({ name: 'Hammer' }),
      photos: [ingestPhoto({ id: 'h1' })],
    });
    const saved = { ...DRILL, status: 'ACCEPTED' as const, photos: [] };
    rerender(
      <IngestReview batch={batch([ingestItem({ id: 'i1', status: 'ACCEPTED' }), saved, next])} />,
    );
    expect(await screen.findByRole('heading', { name: 'Item 3' })).toBeInTheDocument();
    expect(screen.getByLabelText('Name')).toHaveValue('Hammer');
  });

  it('a failed item shows its error with Retry and Skip', async () => {
    const user = userEvent.setup();
    renderReview(
      batch([
        ingestItem({
          id: 'bad',
          status: 'FAILED',
          error: 'No photo could be described',
          photos: [ingestPhoto({ id: 'x' })],
        }),
      ]),
    );
    expect(await screen.findByText('No photo could be described')).toBeInTheDocument();
    expect(screen.queryByRole('form')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Retry' }));
    expect(mutations.retryItem).toHaveBeenCalledWith('bad');
    await user.click(screen.getByRole('button', { name: 'Skip' }));
    expect(mutations.skipItem).toHaveBeenCalledWith('bad');
  });

  it('waits while nothing is ready yet', () => {
    renderReview(
      ingestBatch({
        id: 'b1',
        status: 'PROCESSING',
        items: [ingestItem({ id: 'i1', status: 'ANALYSING' })],
      }),
    );
    expect(screen.getByRole('status')).toHaveTextContent('Analysing your photos…');
    expect(screen.queryByRole('form')).not.toBeInTheDocument();
  });
});
