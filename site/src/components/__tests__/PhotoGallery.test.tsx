import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { PhotoGallery } from '../PhotoGallery';
import { photoRef } from 'test/photoFixtures';

const remove = vi.fn(async () => true);
const setPrimary = vi.fn(async () => null);
vi.mock('hooks/useAttachmentMutations', () => ({
  useDeleteAttachment: () => ({ remove, loading: false }),
  useSetPrimaryPhoto: () => ({ setPrimary, loading: false }),
}));

beforeEach(() => vi.clearAllMocks());

const PHOTOS = [
  photoRef({ id: 'p1', title: 'Front', primary: true }),
  photoRef({ id: 'p2', title: 'Side' }),
  photoRef({ id: 'p3', title: 'Back' }),
];

const tile = (title: string) => screen.getByRole('listitem', { name: title });

describe('PhotoGallery', () => {
  it('renders a 300 thumbnail per photo, badging only the primary one', () => {
    render(<PhotoGallery photos={PHOTOS} />);
    const list = screen.getByRole('list', { name: 'Photos' });
    expect(within(list).getAllByRole('listitem')).toHaveLength(3);
    expect(within(tile('Side')).getByRole('img', { name: 'Side' })).toHaveAttribute(
      'src',
      '/attachments/p2/thumb/300?v=abc',
    );
    expect(within(tile('Front')).getByText('Primary')).toBeInTheDocument();
    expect(within(tile('Side')).queryByText('Primary')).not.toBeInTheDocument();
    expect(within(tile('Back')).queryByText('Primary')).not.toBeInTheDocument();
  });

  it('says so when there are no photos', () => {
    render(<PhotoGallery photos={[]} />);
    expect(screen.getByText('No photos yet.')).toBeInTheDocument();
    expect(screen.queryByRole('list')).not.toBeInTheDocument();
  });

  it('Make primary calls setPrimary, and is not offered on the primary photo', async () => {
    const user = userEvent.setup();
    render(<PhotoGallery photos={PHOTOS} />);
    expect(
      screen.queryByRole('button', { name: 'Make Front the primary photo' }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Make Side the primary photo' }));
    expect(setPrimary).toHaveBeenCalledWith('p2');
  });

  it('Delete asks first, then calls remove and closes the dialog', async () => {
    const user = userEvent.setup();
    render(<PhotoGallery photos={PHOTOS} />);
    await user.click(screen.getByRole('button', { name: 'Delete Side' }));
    expect(remove).not.toHaveBeenCalled();
    const dialog = screen.getByRole('dialog', { name: 'Delete Side?' });
    await user.click(within(dialog).getByRole('button', { name: 'Delete photo' }));
    expect(remove).toHaveBeenCalledWith('p2');
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it('Cancel in the delete dialog removes nothing', async () => {
    const user = userEvent.setup();
    render(<PhotoGallery photos={PHOTOS} />);
    await user.click(screen.getByRole('button', { name: 'Delete Back' }));
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(remove).not.toHaveBeenCalled();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('clicking a thumbnail opens the lightbox at that photo; Escape returns focus to it', async () => {
    const user = userEvent.setup();
    render(<PhotoGallery photos={PHOTOS} />);
    const thumb = screen.getByRole('button', { name: 'View Side' });
    await user.click(thumb);
    const lightbox = screen.getByRole('dialog', { name: 'Side' });
    expect(within(lightbox).getByRole('img')).toHaveAttribute(
      'src',
      '/attachments/p2/thumb/1200?v=abc',
    );
    expect(lightbox).toHaveTextContent('2 of 3');
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(thumb).toHaveFocus();
  });
});
