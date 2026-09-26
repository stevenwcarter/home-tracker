import { describe, it, expect } from 'vitest';
import { isGalleryPhoto } from '../photos';
import { photoRef } from 'test/photoFixtures';

describe('isGalleryPhoto', () => {
  it('takes a photo with a thumbnail', () => {
    expect(isGalleryPhoto(photoRef({ id: 'p1' }))).toBe(true);
  });

  it('leaves out a photo without a thumbnail and an image of another kind', () => {
    expect(isGalleryPhoto(photoRef({ id: 'h1', thumbnailUrl: null }))).toBe(false);
    expect(isGalleryPhoto(photoRef({ id: 'r1', kind: 'RECEIPT' }))).toBe(false);
  });
});
