import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { PhotoUploader } from '../PhotoUploader';
import type { UploadProgress } from 'hooks/useUploadPhoto';

const upload = vi.fn(async () => []);
const hookState: { uploading: boolean; progress: UploadProgress[] } = {
  uploading: false,
  progress: [],
};
const useUploadPhoto = vi.fn((entity: { id: string; parentId: string | null }) => {
  void entity;
  return { upload, ...hookState };
});
vi.mock('hooks/useUploadPhoto', () => ({
  useUploadPhoto: (entity: { id: string; parentId: string | null }) => useUploadPhoto(entity),
}));

beforeEach(() => {
  vi.clearAllMocks();
  hookState.uploading = false;
  hookState.progress = [];
});

const DRILL = { id: 'drill', parentId: 'garage' };
const jpeg = (name: string) => new File(['jpeg'], name, { type: 'image/jpeg' });

const renderUploader = () =>
  render(
    <PhotoUploader entity={DRILL}>
      <p>gallery goes here</p>
    </PhotoUploader>,
  );

describe('PhotoUploader', () => {
  it('uploads for the given entity', () => {
    renderUploader();
    expect(useUploadPhoto).toHaveBeenCalledWith(DRILL);
    expect(screen.getByText('gallery goes here')).toBeInTheDocument();
  });

  it('offers an image-only, multiple file input behind an Add photos button', async () => {
    const user = userEvent.setup();
    renderUploader();
    const input = screen.getByLabelText('Photo files');
    expect(input).toHaveAttribute('type', 'file');
    expect(input).toHaveAttribute('accept', 'image/jpeg,image/png,image/gif,image/webp');
    expect(input).toHaveAttribute('multiple');
    const click = vi.spyOn(input, 'click');
    await user.click(screen.getByRole('button', { name: 'Add photos' }));
    expect(click).toHaveBeenCalledTimes(1);
  });

  it('selecting two files calls upload once with both, then clears the input', async () => {
    const user = userEvent.setup();
    renderUploader();
    const files = [jpeg('front.jpg'), jpeg('side.jpg')];
    const input = screen.getByLabelText<HTMLInputElement>('Photo files');
    await user.upload(input, files);
    expect(upload).toHaveBeenCalledTimes(1);
    expect(upload).toHaveBeenCalledWith(files);
    // Cleared, so choosing the same file again still fires a change.
    expect(input.value).toBe('');
  });

  it('dropping files onto the region calls upload with them', () => {
    renderUploader();
    const zone = screen.getByRole('group', { name: 'Photo upload' });
    const files = [jpeg('front.jpg'), jpeg('side.jpg')];
    fireEvent.dragOver(zone, { dataTransfer: { files, types: ['Files'] } });
    expect(zone).toHaveAttribute('data-dragging', 'true');
    fireEvent.drop(zone, { dataTransfer: { files, types: ['Files'] } });
    expect(upload).toHaveBeenCalledWith(files);
    expect(zone).not.toHaveAttribute('data-dragging');
  });

  it('ignores a drop with no files', () => {
    renderUploader();
    const zone = screen.getByRole('group', { name: 'Photo upload' });
    fireEvent.drop(zone, { dataTransfer: { files: [], types: ['text/plain'] } });
    expect(upload).not.toHaveBeenCalled();
  });

  it('shows each file of the batch with its status and any error', () => {
    const [a, b, c, d] = ['a.jpg', 'b.jpg', 'c.jpg', 'd.jpg'].map(jpeg);
    hookState.uploading = true;
    hookState.progress = [
      { file: a, status: 'done' },
      { file: b, status: 'error', error: 'File is larger than 25 MB' },
      { file: c, status: 'uploading' },
      { file: d, status: 'queued' },
    ];
    renderUploader();
    const list = screen.getByRole('list', { name: 'Upload status' });
    const row = (name: string) => within(list).getByText(name).closest('li') as HTMLElement;
    expect(row('a.jpg')).toHaveTextContent('Uploaded');
    expect(row('b.jpg')).toHaveTextContent('File is larger than 25 MB');
    expect(row('c.jpg')).toHaveTextContent('Uploading');
    expect(row('d.jpg')).toHaveTextContent('Waiting');
    expect(screen.getByRole('button', { name: 'Uploading…' })).toBeDisabled();
  });

  it('has no status list before the first upload', () => {
    renderUploader();
    expect(screen.queryByRole('list', { name: 'Upload status' })).not.toBeInTheDocument();
  });
});
