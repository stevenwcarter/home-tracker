import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, within } from '@testing-library/react';
import { GET_ENTITY, GET_SUMMARY } from 'hooks/queries';
import { entityDetail, listItem, locationSummary, SUMMARY } from 'test/entityFixtures';
import { renderRoute } from 'test/renderRoute';
import { AttachmentRef } from 'types/entity';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const summaryMock = { request: { query: GET_SUMMARY }, result: { data: { summary: SUMMARY } } };
const entityMock = (id: string, entity: unknown) => ({
  request: { query: GET_ENTITY, variables: { id } },
  result: { data: { entity } },
});

const attachment = (overrides: Partial<AttachmentRef> & Pick<AttachmentRef, 'id'>) =>
  ({
    __typename: 'Attachment',
    kind: 'PHOTO',
    primary: false,
    title: `${overrides.id}.jpg`,
    mimeType: 'image/jpeg',
    url: `/attachments/${overrides.id}?v=abc`,
    thumbnailUrl: `/attachments/${overrides.id}/thumb/500?v=abc`,
    ...overrides,
  }) as AttachmentRef;

const photo = attachment({ id: 'p1', primary: true, title: 'Front' });
const manual = attachment({
  id: 'm1',
  kind: 'MANUAL',
  title: 'Manual.pdf',
  mimeType: 'application/pdf',
  thumbnailUrl: null,
});

const drill = entityDetail({
  id: 'drill',
  name: 'Drill',
  description: 'Cordless',
  parentId: 'garage',
  parent: locationSummary({ id: 'garage', name: 'Garage', parentId: 'house' }),
  ancestors: [
    locationSummary({ id: 'house', name: 'House' }),
    locationSummary({ id: 'garage', name: 'Garage', parentId: 'house' }),
  ],
  assetId: '000-001',
  quantity: 2,
  manufacturer: 'Makita',
  serialNumber: null,
  modelNumber: '',
  purchaseDate: '2021-10-23',
  purchaseFrom: 'Hardware store',
  purchasePriceCents: 4999,
  insured: true,
  tags: [{ __typename: 'Tag', id: 't1', name: 'Power tools', color: null } as never],
  attachments: [photo, manual],
  primaryPhoto: photo,
  fields: [
    {
      __typename: 'EntityField',
      id: 'f1',
      name: 'Voltage',
      kind: 'TEXT',
      textValue: '18V',
      numberValue: null,
      booleanValue: false,
      timeValue: null,
    } as never,
  ],
});

describe('ItemPage', () => {
  it('shows breadcrumbs, heading and the large primary photo', async () => {
    renderRoute('/items/drill', [entityMock('drill', drill), summaryMock]);
    expect(screen.getByLabelText('Loading item')).toBeInTheDocument();
    expect(await screen.findByRole('heading', { level: 1, name: 'Drill' })).toBeInTheDocument();
    expect(screen.getByRole('navigation', { name: 'Breadcrumb' })).toHaveTextContent(
      'Home/House/Garage/Drill',
    );
    expect(screen.getByRole('img', { name: 'Front' })).toHaveAttribute(
      'src',
      '/attachments/p1/thumb/1200?v=abc',
    );
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('lists the non-empty details only', async () => {
    renderRoute('/items/drill', [entityMock('drill', drill), summaryMock]);
    const details = await screen.findByRole('region', { name: 'Details' });
    const valueOf = (label: string) => within(details).getByText(label).nextElementSibling;
    expect(valueOf('Asset ID')).toHaveTextContent('000-001');
    expect(valueOf('Type')).toHaveTextContent('Tool');
    expect(valueOf('Quantity')).toHaveTextContent('2');
    expect(valueOf('Manufacturer')).toHaveTextContent('Makita');
    expect(valueOf('Purchase date')).toHaveTextContent('Oct 23, 2021');
    expect(valueOf('Purchased from')).toHaveTextContent('Hardware store');
    expect(await within(details).findByText('$49.99')).toBeInTheDocument();
    expect(valueOf('Insured')).toHaveTextContent('Yes');
    expect(valueOf('Description')).toHaveTextContent('Cordless');
    for (const absent of ['Serial number', 'Model', 'Warranty', 'Sold', 'Notes']) {
      expect(within(details).queryByText(absent)).not.toBeInTheDocument();
    }
  });

  it('shows tags, custom fields, other attachments and timestamps', async () => {
    renderRoute('/items/drill', [entityMock('drill', drill), summaryMock]);
    expect(await screen.findByRole('list', { name: 'Tags' })).toHaveTextContent('Power tools');
    const fields = screen.getByRole('region', { name: 'Custom fields' });
    expect(within(fields).getByText('Voltage').nextElementSibling).toHaveTextContent('18V');
    const attachments = screen.getByRole('region', { name: 'Attachments' });
    expect(within(attachments).getByRole('link', { name: 'Manual.pdf' })).toHaveAttribute(
      'href',
      '/attachments/m1?v=abc',
    );
    // The primary photo is already shown large, so it is not repeated as an attachment.
    expect(within(attachments).queryByRole('link', { name: /Front/ })).not.toBeInTheDocument();
    expect(screen.getByText(/Created Jan 1, 2026/)).toBeInTheDocument();
    expect(screen.getByText(/Updated Jan 2, 2026/)).toBeInTheDocument();
  });

  it('falls back to the first image attachment when there is no primary photo', async () => {
    const other = attachment({ id: 'p2', title: 'Side' });
    const noPrimary = { ...drill, primaryPhoto: null, attachments: [manual, other] };
    renderRoute('/items/drill', [entityMock('drill', noPrimary), summaryMock]);
    expect(await screen.findByRole('img', { name: 'Side' })).toHaveAttribute(
      'src',
      '/attachments/p2/thumb/1200?v=abc',
    );
  });

  it('redirects to the location page when the entity is a location', async () => {
    const garage = entityDetail(
      { id: 'garage', name: 'Garage', items: [listItem({ id: 'saw', name: 'Saw' })] },
      true,
    );
    renderRoute('/items/garage', [entityMock('garage', garage), summaryMock]);
    expect(await screen.findByRole('heading', { level: 1, name: 'Garage' })).toBeInTheDocument();
    expect(screen.getByTestId('current-path')).toHaveTextContent(/^\/locations\/garage$/);
  });

  it('shows Not found when the item does not exist', async () => {
    renderRoute('/items/missing', [entityMock('missing', null), summaryMock]);
    expect(await screen.findByRole('heading', { level: 1, name: 'Not found' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Back to home' })).toHaveAttribute('href', '/');
  });
});
