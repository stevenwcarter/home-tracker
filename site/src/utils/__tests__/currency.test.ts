import { describe, it, expect } from 'vitest';
import { formatCents } from '../currency';

describe('formatCents', () => {
  it('formats whole and fractional dollars', () => {
    expect(formatCents(1234567, 'USD', 'en-US')).toBe('$12,345.67');
    expect(formatCents(0, 'USD', 'en-US')).toBe('$0.00');
  });

  it('respects the currency code', () => {
    expect(formatCents(1999, 'EUR', 'en-US')).toBe('€19.99');
  });

  it('never throws on an unknown currency code', () => {
    expect(formatCents(100, 'NOPE', 'en-US')).toBe('1.00 NOPE');
  });
});
