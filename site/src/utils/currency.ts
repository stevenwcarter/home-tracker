/**
 * Formats integer minor units as money. Falls back to "1.00 CODE" for unknown codes.
 * Defaults `locale` to 'en-US' rather than following the runner's or browser's default
 * locale: currency is an app-level setting, so display formatting must be deterministic.
 * A user-locale setting can be added later.
 */
export function formatCents(cents: number, currency: string, locale = 'en-US'): string {
  const amount = cents / 100;
  try {
    return new Intl.NumberFormat(locale, { style: 'currency', currency }).format(amount);
  } catch {
    return `${amount.toFixed(2)} ${currency}`;
  }
}
