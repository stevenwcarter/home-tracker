import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, within } from '@testing-library/react';
import type { MockedResponse } from '@apollo/client/testing';
import { GET_INGEST_BATCH } from 'hooks/queries';
import { entityDetail, locationSummary } from 'test/entityFixtures';
import {
  ENTITY_TYPES,
  entityTypeWire,
  locationsMock,
  tagsMock,
  typesMock,
} from 'test/formFixtures';
import { ingestBatch, ingestItem, ingestPhoto, ingestSuggestion } from 'test/ingestFixtures';
import { entityMock } from 'test/pageMocks';
import { renderRoute } from 'test/renderRoute';
import { ITEM_TYPE_ID } from 'types/builtIns';
import type { IngestBatch } from 'types/ingest';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const garage = entityDetail(
  {
    id: 'garage',
    name: 'Garage',
    parentId: 'house',
    ancestors: [locationSummary({ id: 'house', name: 'House' })],
  },
  true,
);

const batchMock = (batch: IngestBatch | null, id = batch?.id ?? 'b1'): MockedResponse => ({
  request: { query: GET_INGEST_BATCH, variables: { id } },
  result: { data: { ingestBatch: batch } },
  maxUsageCount: Number.POSITIVE_INFINITY,
});

const lookups = () => [
  typesMock([...ENTITY_TYPES, entityTypeWire(ITEM_TYPE_ID, 'Item', false)]),
  tagsMock(),
  locationsMock(),
];

const photo = ingestPhoto({ id: 'p1', status: 'DESCRIBED', suggestedKind: 'PHOTO' });

const renderBatch = (batch: IngestBatch, extra: MockedResponse[] = []) =>
  renderRoute(`/ingest/${batch.id}`, [batchMock(batch), entityMock('garage', garage), ...extra]);

describe('IngestPage', () => {
  it('shows the heading and a breadcrumb through the parent', async () => {
    renderBatch(ingestBatch({ id: 'b1', items: [ingestItem({ id: 'i1' })] }));
    expect(
      await screen.findByRole('heading', { level: 1, name: 'Add items with AI' }),
    ).toBeInTheDocument();
    const crumbs = screen.getByRole('navigation', { name: 'Breadcrumb' });
    expect(await within(crumbs).findByRole('link', { name: 'Garage' })).toHaveAttribute(
      'href',
      '/locations/garage',
    );
    expect(crumbs).toHaveTextContent('Home/House/Garage/Add items with AI');
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('collects photos while the batch is collecting', async () => {
    renderBatch(ingestBatch({ id: 'b1', items: [ingestItem({ id: 'i1', photos: [photo] })] }));
    expect(await screen.findByRole('region', { name: 'Item 1' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Submit for analysis' })).toBeEnabled();
    expect(screen.queryByRole('list', { name: 'Progress' })).not.toBeInTheDocument();
  });

  it('shows progress and the waiting state while processing', async () => {
    renderBatch(
      ingestBatch({
        id: 'b1',
        status: 'PROCESSING',
        items: [ingestItem({ id: 'i1', status: 'ANALYSING', photos: [photo] })],
      }),
    );
    expect(await screen.findByRole('list', { name: 'Progress' })).toHaveTextContent(
      'Item 1: Analysing',
    );
    expect(screen.getByText('Analysing your photos…')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Submit for analysis' })).not.toBeInTheDocument();
  });

  it('shows progress and the review form while reviewing', async () => {
    renderBatch(
      ingestBatch({
        id: 'b1',
        status: 'REVIEWING',
        items: [
          ingestItem({
            id: 'i1',
            status: 'READY',
            suggestion: ingestSuggestion({ name: 'Drill' }),
            photos: [photo],
          }),
        ],
      }),
      lookups(),
    );
    expect(await screen.findByRole('list', { name: 'Progress' })).toHaveTextContent(
      'Item 1: Ready',
    );
    expect(await screen.findByRole('button', { name: 'Save item' })).toBeInTheDocument();
    expect(screen.getByLabelText('Name')).toHaveValue('Drill');
  });

  it('shows the summary once done', async () => {
    renderBatch(
      ingestBatch({
        id: 'b1',
        status: 'DONE',
        items: [ingestItem({ id: 'i1', status: 'SKIPPED' })],
      }),
    );
    expect(await screen.findByRole('button', { name: 'Add more items' })).toBeInTheDocument();
    expect(await screen.findByRole('link', { name: 'Back to Garage' })).toHaveAttribute(
      'href',
      '/locations/garage',
    );
    expect(screen.queryByRole('list', { name: 'Progress' })).not.toBeInTheDocument();
  });

  it('leads back home for a batch started from no entity', async () => {
    renderRoute('/ingest/b1', [
      batchMock(
        ingestBatch({
          id: 'b1',
          parentId: null,
          status: 'DONE',
          items: [ingestItem({ id: 'i1', status: 'SKIPPED' })],
        }),
      ),
    ]);
    expect(await screen.findByRole('link', { name: 'Back to Home' })).toHaveAttribute('href', '/');
    expect(screen.getByRole('navigation', { name: 'Breadcrumb' })).toHaveTextContent(
      'Home/Add items with AI',
    );
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('shows no breadcrumb or back link until the parent has loaded', async () => {
    const done = ingestBatch({
      id: 'b1',
      status: 'DONE',
      items: [ingestItem({ id: 'i1', status: 'SKIPPED' })],
    });
    renderRoute('/ingest/b1', [batchMock(done), { ...entityMock('garage', garage), delay: 200 }]);
    expect(await screen.findByRole('button', { name: 'Add more items' })).toBeInTheDocument();
    expect(screen.queryByRole('navigation', { name: 'Breadcrumb' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: /Back to/ })).not.toBeInTheDocument();
    expect(await screen.findByRole('link', { name: 'Back to Garage' })).toHaveAttribute(
      'href',
      '/locations/garage',
    );
  });

  it('leads home when the parent no longer exists', async () => {
    const done = ingestBatch({
      id: 'b1',
      status: 'DONE',
      items: [ingestItem({ id: 'i1', status: 'SKIPPED' })],
    });
    renderRoute('/ingest/b1', [batchMock(done), entityMock('garage', null)]);
    expect(await screen.findByRole('link', { name: 'Back to Home' })).toHaveAttribute('href', '/');
  });

  it('shows Not found for an unknown batch', async () => {
    renderRoute('/ingest/nope', [batchMock(null, 'nope')]);
    expect(await screen.findByRole('heading', { level: 1, name: 'Not found' })).toBeInTheDocument();
    expect(screen.getByText("That batch doesn't exist.")).toBeInTheDocument();
  });

  it('has no fixed widths anywhere on the collect screen (phone width)', async () => {
    const { container } = renderBatch(
      ingestBatch({
        id: 'b1',
        items: [ingestItem({ id: 'i1', photos: [photo] }), ingestItem({ id: 'i2', position: 3 })],
      }),
    );
    await screen.findByRole('region', { name: 'Item 2' });
    for (const element of container.querySelectorAll('*')) {
      expect(element.getAttribute('class') ?? '').not.toMatch(/(^|\s)(min-)?w-([1-9]|\[|px)/);
    }
  });
});
