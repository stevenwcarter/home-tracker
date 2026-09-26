import { useSummary } from './useSummary';

const DEFAULT_CURRENCY = 'USD';

/** The app's display currency (a server setting, delivered with the summary). */
export const useCurrency = (): string => useSummary().summary?.currency ?? DEFAULT_CURRENCY;
