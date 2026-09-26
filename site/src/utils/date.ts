const LOCALE = 'en-US';
const DATE_OPTIONS: Intl.DateTimeFormatOptions = {
  month: 'short',
  day: 'numeric',
  year: 'numeric',
};
const DATE_ONLY = /^(\d{4})-(\d{2})-(\d{2})$/;

/**
 * Formats a date as a short `en-US` calendar date ("Oct 23, 2021").
 *
 * A date-only string (`YYYY-MM-DD`, as the API sends `purchaseDate` and
 * friends) is a calendar date, not an instant: `new Date('2021-10-23')` would
 * read it as UTC midnight and show the previous day west of Greenwich, so it
 * is built as a local date instead. Anything unparseable is returned as is.
 */
export function formatDate(value: string): string {
  const dateOnly = DATE_ONLY.exec(value);
  const date = dateOnly
    ? new Date(Number(dateOnly[1]), Number(dateOnly[2]) - 1, Number(dateOnly[3]))
    : new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleDateString(LOCALE, DATE_OPTIONS);
}

/** Formats an ISO timestamp in local time ("Jan 1, 2026, 12:00 PM"); unparseable input is returned as is. */
export function formatDateTime(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : date.toLocaleString(LOCALE, { ...DATE_OPTIONS, hour: 'numeric', minute: '2-digit' });
}
