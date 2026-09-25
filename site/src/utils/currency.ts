/** Formats integer minor units as money. Falls back to "1.00 CODE" for unknown codes. */
export function formatCents(cents: number, currency: string, locale?: string): string {
  const amount = cents / 100;
  try {
    return new Intl.NumberFormat(locale, { style: 'currency', currency }).format(amount);
  } catch {
    return `${amount.toFixed(2)} ${currency}`;
  }
}
