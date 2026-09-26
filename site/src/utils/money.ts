/** A money string a form cannot turn into cents; `message` is fit to show inline. */
export class MoneyParseError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'MoneyParseError';
  }
}

/** The largest amount the API's `Int` cents fields hold. */
const MAX_CENTS = 2 ** 31 - 1;

/**
 * Whole units either ungrouped (`1234`) or grouped by commas in threes
 * (`1,234`), then an optional `.` with up to two decimals. Either side of the
 * point may be empty (`12.`, `.5`), not both.
 */
const MONEY = /^(\d+|\d{1,3}(?:,\d{3})+)?(?:\.(\d{0,2}))?$/;

/**
 * Parses an amount in major units (`"12.5"`, `"1,234.56"`) to cents. Blank is
 * zero. Signs, exponents, currency symbols, more than two decimals and
 * misplaced commas are refused with a `MoneyParseError`.
 */
export function parseMoney(input: string): number {
  const text = input.trim();
  if (text === '') return 0;
  const match = MONEY.exec(text);
  const [, whole = '', fraction = ''] = match ?? [];
  if (!match || (whole === '' && fraction === '')) {
    throw new MoneyParseError('Enter an amount like 12.50');
  }
  const cents = Number(whole.replaceAll(',', '') || '0') * 100 + Number(fraction.padEnd(2, '0'));
  if (cents > MAX_CENTS) throw new MoneyParseError('Amount is too large');
  return cents;
}

/** Formats cents for a money input: major units, two decimals, no grouping (`1250` → `"12.50"`). */
export function formatMoneyInput(cents: number): string {
  const sign = cents < 0 ? '-' : '';
  const abs = Math.abs(cents);
  return `${sign}${Math.floor(abs / 100)}.${String(abs % 100).padStart(2, '0')}`;
}
