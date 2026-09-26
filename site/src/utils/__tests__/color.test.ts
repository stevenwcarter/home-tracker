import { describe, it, expect } from 'vitest';
import { isHexColor } from '../color';

describe('isHexColor', () => {
  it.each(['#abc', '#ABC', '#4a90d9', '#FF8800', '#4a90d9cc', '#AABBCCDD'])(
    'accepts %s',
    (value) => {
      expect(isHexColor(value)).toBe(true);
    },
  );

  it.each([
    null,
    undefined,
    '',
    'red',
    '#abcd',
    '#12345',
    '#1234567',
    '#123456789',
    '#ggg',
    '4a90d9',
    'url(https://example.com/x.png)',
    '#fff; background: red',
  ])('rejects %s', (value) => {
    expect(isHexColor(value)).toBe(false);
  });
});
