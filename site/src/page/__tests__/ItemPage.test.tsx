import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {
  DELETE_ATTACHMENT,
  DELETE_ENTITY,
  GET_LOCATIONS,
  GET_ROOT_ITEMS,
  GET_ENTITY,
  GET_SUMMARY,
  SET_PRIMARY_PHOTO,
} from 'hooks/queries';
import { entityDetail, listItem, locationSummary, SUMMARY } from 'test/entityFixtures';
import { typesMock } from 'test/formFixtures';
import { entityMock, spiedMock, summaryMock as summary } from 'test/pageMocks';
import { renderRoute } from 'test/renderRoute';
import { AttachmentKind, AttachmentRef, EntityFieldRef, FieldKind } from 'types/entity';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

const summaryMock = summary();

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

/** The large photo beside the details (the gallery shows the same photo again at 300). */
const featured = () =>
  within(screen.getByRole('figure', { name: 'Featured photo' })).getByRole('img');
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
  it('renders a value for every FieldKind and every AttachmentKind', async () => {
    const field = (kind: FieldKind, values: Partial<EntityFieldRef>) =>
      ({
        __typename: 'EntityField',
        id: `f-${kind}`,
        name: `Field ${kind}`,
        kind,
        textValue: null,
        numberValue: null,
        booleanValue: false,
        timeValue: null,
        ...values,
      }) as EntityFieldRef;
    const nonPhotoKinds: AttachmentKind[] = ['MANUAL', 'WARRANTY', 'ATTACHMENT', 'RECEIPT'];
    const everyKind = entityDetail({
      id: 'drill',
      name: 'Drill',
      fields: [
        field('TEXT', { textValue: '18V' }),
        field('NUMBER', { numberValue: 7 }),
        field('BOOLEAN', { booleanValue: true }),
        field('TIME', { timeValue: '2026-03-04T12:00:00Z' }),
      ],
      attachments: [
        photo,
        ...nonPhotoKinds.map((kind) =>
          attachment({ id: kind, kind, title: `${kind} file`, thumbnailUrl: null }),
        ),
      ],
      primaryPhoto: photo,
    });
    renderRoute('/items/drill', [entityMock('drill', everyKind), summaryMock]);
    const fields = await screen.findByRole('region', { name: 'Custom fields' });
    const valueOf = (label: string) => within(fields).getByText(label).nextElementSibling;
    expect(valueOf('Field TEXT')).toHaveTextContent('18V');
    expect(valueOf('Field NUMBER')).toHaveTextContent('7');
    expect(valueOf('Field BOOLEAN')).toHaveTextContent('Yes');
    expect(valueOf('Field TIME')).toHaveTextContent('Mar 4, 2026');
    expect(featured()).toHaveAccessibleName('Front');
    const attachments = screen.getByRole('region', { name: 'Attachments' });
    for (const kind of nonPhotoKinds) {
      expect(within(attachments).getByRole('link', { name: `${kind} file` })).toBeInTheDocument();
    }
  });

  it('shows breadcrumbs, heading and the large primary photo', async () => {
    renderRoute('/items/drill', [entityMock('drill', drill), summaryMock]);
    expect(screen.getByLabelText('Loading item')).toBeInTheDocument();
    expect(await screen.findByRole('heading', { level: 1, name: 'Drill' })).toBeInTheDocument();
    expect(screen.getByRole('navigation', { name: 'Breadcrumb' })).toHaveTextContent(
      'Home/House/Garage/Drill',
    );
    expect(featured()).toHaveAccessibleName('Front');
    expect(featured()).toHaveAttribute('src', '/attachments/p1/thumb/1200?v=abc');
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
    await screen.findByRole('figure', { name: 'Featured photo' });
    expect(featured()).toHaveAccessibleName('Side');
    expect(featured()).toHaveAttribute('src', '/attachments/p2/thumb/1200?v=abc');
  });

  it('lists an attachment without a thumbnail by title, even when its MIME type is an image', async () => {
    const heic = attachment({
      id: 'h1',
      title: 'Receipt.heic',
      mimeType: 'image/heic',
      thumbnailUrl: null,
    });
    const heicOnly = { ...drill, primaryPhoto: null, attachments: [heic] };
    renderRoute('/items/drill', [entityMock('drill', heicOnly), summaryMock]);
    const attachments = await screen.findByRole('region', { name: 'Attachments' });
    expect(within(attachments).getByRole('link', { name: 'Receipt.heic' })).toHaveAttribute(
      'href',
      '/attachments/h1?v=abc',
    );
    expect(screen.queryByRole('img')).not.toBeInTheDocument();
    expect(screen.queryByText('No photo')).not.toBeInTheDocument();
  });

  it('shows the contents of an item that holds other items', async () => {
    const toolbox = entityDetail({
      id: 'toolbox',
      name: 'Toolbox',
      items: [listItem({ id: 'bit-set', name: 'Bit set' })],
    });
    renderRoute('/items/toolbox', [entityMock('toolbox', toolbox), summaryMock]);
    const contents = await screen.findByRole('region', { name: 'Contents' });
    expect(within(contents).getByRole('link', { name: 'Bit set' })).toHaveAttribute(
      'href',
      '/items/bit-set',
    );
  });

  it('has no Contents section when the item holds nothing', async () => {
    renderRoute('/items/drill', [entityMock('drill', drill), summaryMock]);
    await screen.findByRole('heading', { level: 1, name: 'Drill' });
    expect(screen.queryByRole('region', { name: 'Contents' })).not.toBeInTheDocument();
  });

  it('redirects to the location page when the entity is a location', async () => {
    const garage = entityDetail(
      { id: 'garage', name: 'Garage', items: [listItem({ id: 'saw', name: 'Saw' })] },
      true,
    );
    renderRoute('/items/garage', [entityMock('garage', garage), summaryMock, typesMock()]);
    expect(await screen.findByRole('heading', { level: 1, name: 'Garage' })).toBeInTheDocument();
    expect(screen.getByTestId('current-path')).toHaveTextContent(/^\/locations\/garage$/);
  });

  it('shows Not found when the item does not exist', async () => {
    renderRoute('/items/missing', [entityMock('missing', null), summaryMock]);
    expect(await screen.findByRole('heading', { level: 1, name: 'Not found' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Back to home' })).toHaveAttribute('href', '/');
  });

  it('links Edit to the edit form', async () => {
    renderRoute('/items/drill', [entityMock('drill', drill), summaryMock]);
    expect(await screen.findByRole('link', { name: 'Edit' })).toHaveAttribute(
      'href',
      '/items/drill/edit',
    );
  });
});

describe('ItemPage delete', () => {
  // Review Focus 5: the parent's cached contents are evicted and refetched, so
  // the location page stops listing the item without a reload.
  it('returns to the parent location, which no longer lists the item', async () => {
    const user = userEvent.setup();
    const garage = entityDetail(
      {
        id: 'garage',
        name: 'Garage',
        items: [listItem({ id: 'drill', name: 'Drill' }), listItem({ id: 'saw', name: 'Saw' })],
      },
      true,
    );
    const deleted = spiedMock(DELETE_ENTITY, { id: 'drill' }, { deleteEntity: true });
    const summaryRefetch = spiedMock(GET_SUMMARY, undefined, { summary: SUMMARY });
    const garageRefetch = spiedMock(
      GET_ENTITY,
      { id: 'garage' },
      { entity: { ...garage, items: [listItem({ id: 'saw', name: 'Saw' })] } },
    );
    renderRoute('/locations/garage', [
      entityMock('garage', garage),
      summaryMock,
      typesMock(),
      entityMock('drill', drill),
      deleted.mock,
      summaryRefetch.mock,
      // The evicted item page refetches itself before it is left.
      entityMock('drill', null),
      garageRefetch.mock,
      // The write evicted the unmounted types list, so the location page reloads it.
      typesMock(),
    ]);

    const items = await screen.findByRole('region', { name: 'Items' });
    await user.click(within(items).getByRole('link', { name: 'Drill' }));
    await user.click(await screen.findByRole('button', { name: 'Delete' }));
    const dialog = screen.getByRole('dialog', { name: 'Delete Drill?' });
    await user.click(within(dialog).getByRole('button', { name: 'Delete item' }));

    await waitFor(() =>
      expect(screen.getByTestId('current-path')).toHaveTextContent(/^\/locations\/garage$/),
    );
    const refreshed = await screen.findByRole('region', { name: 'Items' });
    expect(await within(refreshed).findByRole('link', { name: 'Saw' })).toBeInTheDocument();
    expect(within(refreshed).queryByRole('link', { name: 'Drill' })).not.toBeInTheDocument();
    expect(deleted.result).toHaveBeenCalledTimes(1);
    expect(summaryRefetch.result).toHaveBeenCalledTimes(1);
    expect(garageRefetch.result).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('heading', { name: 'Not found' })).not.toBeInTheDocument();
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('goes home when the item has no parent', async () => {
    const user = userEvent.setup();
    const loose = entityDetail({ id: 'loose', name: 'Loose screw' });
    renderRoute('/items/loose', [
      entityMock('loose', loose),
      summaryMock,
      spiedMock(DELETE_ENTITY, { id: 'loose' }, { deleteEntity: true }).mock,
      summary(),
      entityMock('loose', null),
      { request: { query: GET_LOCATIONS }, result: { data: { locations: [] } } },
      { request: { query: GET_ROOT_ITEMS }, result: { data: { rootItems: [] } } },
    ]);
    await user.click(await screen.findByRole('button', { name: 'Delete' }));
    await user.click(screen.getByRole('button', { name: 'Delete item' }));
    expect(await screen.findByRole('heading', { level: 1, name: 'Home' })).toBeInTheDocument();
    expect(screen.getByTestId('current-path')).toHaveTextContent(/^\/$/);
  });
});

describe('ItemPage attachments', () => {
  const side = attachment({ id: 'p2', title: 'Side' });
  const withSide = { ...drill, attachments: [photo, side, manual] };

  it('offers Make primary on a non-primary photo only', async () => {
    renderRoute('/items/drill', [entityMock('drill', withSide), summaryMock]);
    expect(
      await screen.findByRole('button', { name: 'Make Side the primary photo' }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Make Front the primary photo' }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Make Manual.pdf the primary photo' }),
    ).not.toBeInTheDocument();
  });

  it('Make primary sends setPrimaryPhoto and shows the new hero photo', async () => {
    const user = userEvent.setup();
    const promoted = {
      ...withSide,
      attachments: [{ ...photo, primary: false }, { ...side, primary: true }, manual],
      primaryPhoto: { ...side, primary: true },
    };
    const setPrimary = spiedMock(
      SET_PRIMARY_PHOTO,
      { attachmentId: 'p2' },
      { setPrimaryPhoto: promoted },
    );
    renderRoute('/items/drill', [
      entityMock('drill', withSide),
      summaryMock,
      setPrimary.mock,
      summary(),
      entityMock('drill', promoted),
    ]);
    await user.click(await screen.findByRole('button', { name: 'Make Side the primary photo' }));
    await waitFor(() =>
      expect(featured()).toHaveAttribute('src', '/attachments/p2/thumb/1200?v=abc'),
    );
    expect(featured()).toHaveAccessibleName('Side');
    expect(setPrimary.result).toHaveBeenCalledTimes(1);
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('deletes an attachment after confirming', async () => {
    const user = userEvent.setup();
    const withoutManual = { ...withSide, attachments: [photo, side] };
    const removed = spiedMock(DELETE_ATTACHMENT, { id: 'm1' }, { deleteAttachment: true });
    renderRoute('/items/drill', [
      entityMock('drill', withSide),
      summaryMock,
      removed.mock,
      summary(),
      entityMock('drill', withoutManual),
    ]);
    await user.click(await screen.findByRole('button', { name: 'Delete Manual.pdf' }));
    const dialog = screen.getByRole('dialog', { name: 'Delete Manual.pdf?' });
    await user.click(within(dialog).getByRole('button', { name: 'Delete attachment' }));
    await waitFor(() =>
      expect(screen.queryByRole('link', { name: 'Manual.pdf' })).not.toBeInTheDocument(),
    );
    // The dialog closes once the delete (and its awaited refetches) resolves.
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(removed.result).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId('current-path')).toHaveTextContent(/^\/items\/drill$/);
  });

  it('can delete the hero photo too', async () => {
    renderRoute('/items/drill', [entityMock('drill', drill), summaryMock]);
    expect(await screen.findByRole('button', { name: 'Delete Front' })).toBeInTheDocument();
  });
});

describe('ItemPage photos', () => {
  const side = attachment({ id: 'p2', title: 'Side' });
  const receipt = attachment({ id: 'r1', kind: 'RECEIPT', title: 'Receipt.png' });
  const withPhotos = { ...drill, attachments: [photo, side, manual, receipt] };

  it('shows the gallery of photos and the uploader, keeping the featured photo', async () => {
    renderRoute('/items/drill', [entityMock('drill', withPhotos), summaryMock]);
    const photos = await screen.findByRole('region', { name: 'Photos' });
    const gallery = within(photos).getByRole('list', { name: 'Photos' });
    expect(within(gallery).getAllByRole('listitem')).toHaveLength(2);
    expect(within(gallery).getByRole('img', { name: 'Front' })).toHaveAttribute(
      'src',
      '/attachments/p1/thumb/300?v=abc',
    );
    expect(within(gallery).getByText('Primary')).toBeInTheDocument();
    expect(within(photos).getByRole('button', { name: 'Add photos' })).toBeInTheDocument();
    expect(within(photos).getByRole('group', { name: 'Photo upload' })).toBeInTheDocument();
    // The hero is unchanged: the primary photo, large, linking to the original.
    expect(featured()).toHaveAttribute('src', '/attachments/p1/thumb/1200?v=abc');
    expect(featured().closest('a')).toHaveAttribute('href', '/attachments/p1?v=abc');
    // Non-photo attachments (even an image receipt) stay in the file list.
    const files = screen.getByRole('region', { name: 'Attachments' });
    expect(within(files).getByRole('link', { name: 'Manual.pdf' })).toBeInTheDocument();
    expect(within(files).getByRole('img', { name: 'Receipt.png' })).toBeInTheDocument();
    expect(within(files).queryByRole('img', { name: 'Side' })).not.toBeInTheDocument();
  });

  it('uploads a chosen file and shows it once the entity refetches', async () => {
    const user = userEvent.setup();
    const uploaded = attachment({ id: 'p9', title: 'Back' });
    const refetched = spiedMock(
      GET_ENTITY,
      { id: 'drill' },
      { entity: { ...withPhotos, attachments: [photo, side, uploaded, manual] } },
    );
    renderRoute('/items/drill', [
      entityMock('drill', withPhotos),
      summaryMock,
      refetched.mock,
      summary(),
    ]);
    const input = await screen.findByLabelText('Photo files');
    const file = new File(['jpeg'], 'back.jpg', { type: 'image/jpeg' });
    await user.upload(input, file);

    const gallery = await screen.findByRole('list', { name: 'Photos' });
    expect(await within(gallery).findByRole('img', { name: 'Back' })).toBeInTheDocument();
    expect(refetched.result).toHaveBeenCalledTimes(1);
    const [url, init] = vi.mocked(fetch).mock.calls[0];
    expect(url).toBe('/api/upload/drill');
    expect((init?.body as FormData).get('file')).toBe(file);
    const status = screen.getByRole('list', { name: 'Upload status' });
    expect(within(status).getByText('back.jpg').closest('li')).toHaveTextContent('Uploaded');
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('shows an empty gallery with the uploader when the item has no photos', async () => {
    renderRoute('/items/drill', [
      entityMock('drill', { ...drill, attachments: [manual], primaryPhoto: null }),
      summaryMock,
    ]);
    const photos = await screen.findByRole('region', { name: 'Photos' });
    expect(within(photos).getByText('No photos yet.')).toBeInTheDocument();
    expect(within(photos).getByRole('button', { name: 'Add photos' })).toBeInTheDocument();
    expect(screen.queryByRole('figure', { name: 'Featured photo' })).not.toBeInTheDocument();
  });

  // The route is not keyed by `:id` and a cached entity skips the skeleton, so
  // without a per-entity key one uploader instance would carry its status
  // (and a running batch's disabled state) over to the next entity.
  it("does not carry an upload's status over to another cached entity", async () => {
    const user = userEvent.setup();
    const toolbox = entityDetail({
      id: 'toolbox',
      name: 'Toolbox',
      items: [listItem({ id: 'drill', name: 'Drill' })],
    });
    const drillHere = {
      ...drill,
      parentId: null,
      items: [listItem({ id: 'toolbox', name: 'Toolbox' })],
    };
    renderRoute('/items/toolbox', [
      entityMock('toolbox', toolbox),
      summaryMock,
      entityMock('drill', drillHere),
      entityMock('drill', drillHere),
      summary(),
    ]);
    const contents = await screen.findByRole('region', { name: 'Contents' });
    await user.click(within(contents).getByRole('link', { name: 'Drill' }));
    await screen.findByRole('heading', { level: 1, name: 'Drill' });
    await user.upload(
      screen.getByLabelText('Photo files'),
      new File(['jpeg'], 'front.jpg', { type: 'image/jpeg' }),
    );
    const status = await screen.findByRole('list', { name: 'Upload status' });
    await waitFor(() => expect(status).toHaveTextContent('Uploaded'));

    const drillContents = screen.getByRole('region', { name: 'Contents' });
    await user.click(within(drillContents).getByRole('link', { name: 'Toolbox' }));
    // Toolbox is cached, so it renders at once, with no skeleton in between.
    expect(await screen.findByRole('heading', { level: 1, name: 'Toolbox' })).toBeInTheDocument();
    expect(screen.queryByRole('list', { name: 'Upload status' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Add photos' })).toBeEnabled();
  });
});
