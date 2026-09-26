import { describe, it, expect } from 'vitest';
import { formatDate, formatDateTime } from '../date';

describe('formatDate', () => {
  it('formats a date-only string as a short en-US calendar date', () => {
    expect(formatDate('2021-10-23')).toBe('Oct 23, 2021');
  });

  it('never shifts a date-only string by a day (it is a calendar date, not UTC midnight)', () => {
    expect(formatDate('2021-01-01')).toBe('Jan 1, 2021');
    expect(formatDate('2021-12-31')).toBe('Dec 31, 2021');
  });

  it('returns the input unchanged when it is not a date', () => {
    expect(formatDate('someday')).toBe('someday');
  });
});

describe('formatDateTime', () => {
  it('formats an ISO timestamp with a short date and a time', () => {
    // Noon UTC is the same calendar day in every inhabited time zone.
    const formatted = formatDateTime('2026-01-01T12:00:00Z');
    expect(formatted).toMatch(/^Jan 1, 2026, \d{1,2}:\d{2}\s?[AP]M$/);
  });

  it('returns the input unchanged when it is not a timestamp', () => {
    expect(formatDateTime('never')).toBe('never');
  });
});
