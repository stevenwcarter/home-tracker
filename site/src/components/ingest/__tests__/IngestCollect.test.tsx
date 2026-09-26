import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { IngestCollect } from '../IngestCollect';
import { renderWithApollo } from 'test/formFixtures';
import { ingestBatch, ingestItem, ingestPhoto } from 'test/ingestFixtures';
import type { IngestItem } from 'types/ingest';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));

const mutations = vi.hoisted(() => ({
  addItem: vi.fn(),
  removeItem: vi.fn(),
  removePhoto: vi.fn(),
  submit: vi.fn(),
  deleteBatch: vi.fn(),
}));
vi.mock('hooks/useIngestMutations', () => ({
  useIngestMutations: () => ({ ...mutations, loading: false }),
}));

const navigate = vi.hoisted(() => vi.fn());
vi.mock('react-router-dom', async (original) => ({
  ...(await original<typeof import('react-router-dom')>()),
  useNavigate: () => navigate,
}));

beforeEach(() => vi.clearAllMocks());

const photos = (itemId: string, count: number) =>
  Array.from({ length: count }, (_, index) =>
    ingestPhoto({ id: `${itemId}-p${index + 1}`, position: index }),
  );

/** Review Focus 5: three items (positions with gaps) holding eight photos. */
const ITEMS: IngestItem[] = [
  ingestItem({ id: 'i1', position: 0, photos: photos('i1', 3) }),
  ingestItem({ id: 'i2', position: 4, photos: photos('i2', 3) }),
  ingestItem({ id: 'i3', position: 9, photos: photos('i3', 2) }),
];
const BATCH = ingestBatch({ id: 'b1', items: ITEMS });
const BACK = { name: 'Garage', path: '/locations/garage' };

const renderCollect = (batch = BATCH) =>
  renderWithApollo(<IngestCollect batch={batch} back={BACK} />, []);

const group = (label: string) => screen.getByRole('region', { name: label });

describe('IngestCollect', () => {
  it('shows one group per item, labelled by index, each with its photos', () => {
    renderCollect();
    for (const [index, count] of [3, 3, 2].entries()) {
      const photoList = within(group(`Item ${index + 1}`)).getByRole('list', {
        name: `Photos of Item ${index + 1}`,
      });
      expect(within(photoList).getAllByRole('img')).toHaveLength(count);
    }
    expect(screen.queryByRole('region', { name: 'Item 4' })).not.toBeInTheDocument();
  });

  it('shows each photo at 300 px', () => {
    renderCollect();
    const [first] = within(group('Item 1')).getAllByRole('img');
    expect(first).toHaveAttribute('src', '/ingest/photos/i1-p1/thumb/300?v=abc');
  });

  it('"Next item" adds an item to the batch', async () => {
    const user = userEvent.setup();
    renderCollect();
    await user.click(screen.getByRole('button', { name: 'Next item' }));
    expect(mutations.addItem).toHaveBeenCalledWith('b1');
  });

  it("a photo's remove button removes just that photo", async () => {
    const user = userEvent.setup();
    renderCollect();
    await user.click(
      within(group('Item 2')).getByRole('button', { name: 'Remove photo 2 of Item 2' }),
    );
    expect(mutations.removePhoto).toHaveBeenCalledTimes(1);
    expect(mutations.removePhoto).toHaveBeenCalledWith('i2-p2');
  });

  it('"Remove item" appears only when there is more than one item', () => {
    renderCollect(ingestBatch({ id: 'b1', items: [ITEMS[0]] }));
    expect(screen.queryByRole('button', { name: /Remove item/ })).not.toBeInTheDocument();
  });

  it('"Remove item" removes an empty item at once', async () => {
    const user = userEvent.setup();
    renderCollect(ingestBatch({ id: 'b1', items: [ITEMS[0], ingestItem({ id: 'empty' })] }));
    await user.click(within(group('Item 2')).getByRole('button', { name: 'Remove item' }));
    expect(mutations.removeItem).toHaveBeenCalledWith('empty');
  });

  it('"Remove item" asks first when the item has photos', async () => {
    const user = userEvent.setup();
    renderCollect();
    await user.click(within(group('Item 3')).getByRole('button', { name: 'Remove item' }));
    const dialog = screen.getByRole('dialog', { name: 'Remove Item 3?' });
    expect(mutations.removeItem).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole('button', { name: 'Remove item' }));
    expect(mutations.removeItem).toHaveBeenCalledWith('i3');
  });

  it('"Submit for analysis" is disabled without photos', () => {
    renderCollect(ingestBatch({ id: 'b1', items: [ingestItem({ id: 'i1' })] }));
    expect(screen.getByRole('button', { name: 'Submit for analysis' })).toBeDisabled();
  });

  it('"Submit for analysis" submits the batch', async () => {
    const user = userEvent.setup();
    renderCollect();
    await user.click(screen.getByRole('button', { name: 'Submit for analysis' }));
    expect(mutations.submit).toHaveBeenCalledWith('b1');
  });

  it('"Discard batch" confirms, deletes the batch and goes back', async () => {
    mutations.deleteBatch.mockResolvedValue(true);
    const user = userEvent.setup();
    renderCollect();
    await user.click(screen.getByRole('button', { name: 'Discard batch' }));
    const dialog = screen.getByRole('dialog', { name: 'Discard this batch?' });
    expect(within(dialog).getByRole('button', { name: 'Cancel' })).toHaveFocus();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(mutations.deleteBatch).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: 'Discard batch' }));
    await user.click(
      within(screen.getByRole('dialog')).getByRole('button', { name: 'Discard batch' }),
    );
    expect(mutations.deleteBatch).toHaveBeenCalledWith('b1');
    await waitFor(() => expect(navigate).toHaveBeenCalledWith('/locations/garage'));
  });

  // Review Focus 5: phones transcode HEIC to JPEG only for `image/*` pickers.
  it('each item has "Add photos" and "Take photo", both image/* and never HEIC', () => {
    renderCollect();
    const item = group('Item 1');
    expect(within(item).getByRole('button', { name: 'Add photos' })).toBeInTheDocument();
    expect(within(item).getByRole('button', { name: 'Take photo' })).toBeInTheDocument();
    const files = within(item).getByLabelText('Photo files');
    const camera = within(item).getByLabelText('Camera photo');
    expect(files).toHaveAttribute('accept', 'image/*');
    expect(files).toHaveAttribute('multiple');
    expect(files).not.toHaveAttribute('capture');
    expect(camera).toHaveAttribute('accept', 'image/*');
    expect(camera).toHaveAttribute('capture', 'environment');
    for (const input of [files, camera]) {
      expect(input.getAttribute('accept')).not.toMatch(/heic|heif/i);
    }
  });

  it("stages a photo chosen in an item's group on that item", async () => {
    const urls: string[] = [];
    vi.mocked(fetch).mockImplementation(async (input) => {
      urls.push(String(input));
      return new Response(JSON.stringify({ id: 'new' }), {
        status: 201,
        headers: { 'content-type': 'application/json' },
      });
    });
    const user = userEvent.setup();
    renderCollect();
    const file = new File(['jpeg'], 'drill.jpg', { type: 'image/jpeg' });
    await user.upload(within(group('Item 2')).getByLabelText('Camera photo'), file);
    await waitFor(() => expect(urls).toContain('/api/ingest/items/i2/photos'));
  });

  it('lays the photos out in a wrapping grid with no fixed widths (phone width)', () => {
    const { container } = renderCollect();
    const grid = within(group('Item 1')).getByRole('list', { name: 'Photos of Item 1' });
    expect(grid).toHaveClass('grid', 'grid-cols-3');
    const actions = screen.getByRole('button', { name: 'Next item' }).parentElement;
    expect(actions).toHaveClass('flex-wrap');
    for (const element of container.querySelectorAll('*')) {
      expect(element.getAttribute('class') ?? '').not.toMatch(/(^|\s)(min-)?w-([1-9]|\[|px)/);
      expect((element as HTMLElement).style?.width ?? '').toBe('');
    }
  });
});
