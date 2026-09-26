import { describe, it, expect } from 'vitest';
import { uploadErrorMessage } from '../uploadErrors';

describe('uploadErrorMessage', () => {
  it.each([
    [413, 'File is larger than 25 MB'],
    [415, 'Only JPEG, PNG, GIF and WebP images are supported'],
    [422, 'That file is not a readable image'],
  ])('maps %i to the fixed message, whatever the server said', (status, message) => {
    expect(uploadErrorMessage(status, 'something else')).toBe(message);
    expect(uploadErrorMessage(status, null)).toBe(message);
  });

  it("uses the server's message for any other status", () => {
    expect(uploadErrorMessage(404, 'entity not found')).toBe('entity not found');
    expect(uploadErrorMessage(403, 'read-only')).toBe('read-only');
  });

  it('falls back to the status when the server gave no message', () => {
    expect(uploadErrorMessage(500, null)).toBe('Upload failed (HTTP 500)');
    expect(uploadErrorMessage(400, '')).toBe('Upload failed (HTTP 400)');
  });
});
