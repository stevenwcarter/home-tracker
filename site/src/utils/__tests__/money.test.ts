import { describe, it, expect } from 'vitest';
import { formatMoneyInput, MoneyParseError, parseMoney } from '../money';

describe('parseMoney', () => {
  it.each([
    ['12.5', 1250],
    ['1,234.56', 123456],
    ['', 0],
    ['   ', 0],
    [' 7 ', 700],
    ['12.', 1200],
    ['.5', 50],
    ['0.05', 5],
    ['1234567.89', 123456789],
    ['12,345', 1234500],
  ])('parses %j as %d cents', (input, cents) => {
    expect(parseMoney(input)).toBe(cents);
  });

  it.each(['abc', '1,2,3', '-1', '1e3', '.', '1.234', '12,34', '$5', '1 000', '21474836.48'])(
    'refuses %j',
    (input) => {
      expect(() => parseMoney(input)).toThrow(MoneyParseError);
    },
  );

  it('accepts the largest amount a GraphQL Int holds', () => {
    expect(parseMoney('21474836.47')).toBe(2147483647);
  });
});

describe('formatMoneyInput', () => {
  it.each([
    [1250, '12.50'],
    [0, '0.00'],
    [5, '0.05'],
    [123456, '1234.56'],
  ])('formats %d cents as %j', (cents, text) => {
    expect(formatMoneyInput(cents)).toBe(text);
  });

  it('round-trips through parseMoney', () => {
    for (const cents of [0, 1, 99, 100, 1250, 123456789]) {
      expect(parseMoney(formatMoneyInput(cents))).toBe(cents);
    }
  });
});
