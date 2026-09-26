import { describe, it, expect } from 'vitest';
import { formatDate, formatDateTime, timeAgo } from '../date';

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

describe('timeAgo', () => {
  const now = new Date('2026-09-26T12:00:00Z');

  it.each([
    ['2026-09-26T11:59:30Z', 'just now'],
    ['2026-09-26T11:59:00Z', '1 minute ago'],
    ['2026-09-26T11:15:00Z', '45 minutes ago'],
    ['2026-09-26T11:00:00Z', '1 hour ago'],
    ['2026-09-26T02:00:00Z', '10 hours ago'],
    ['2026-09-25T12:00:00Z', '1 day ago'],
    ['2026-09-19T12:00:00Z', '7 days ago'],
    // A clock a little ahead of ours is still "just now", never "in the future".
    ['2026-09-26T12:00:30Z', 'just now'],
  ])('%s is %s', (value, expected) => {
    expect(timeAgo(value, now)).toBe(expected);
  });

  it('returns unparseable input as is', () => {
    expect(timeAgo('not a date', now)).toBe('not a date');
  });
});
