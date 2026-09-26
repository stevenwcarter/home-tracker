import { useQuery } from '@apollo/client/react';
import { Summary } from 'types/summary';
import { GET_SUMMARY } from './queries';

const DEFAULT_CURRENCY = 'USD';

/**
 * The app's display currency (a server setting, delivered with the summary).
 * Reads the summary without toasting: a page that only needs the currency
 * should not report a summary failure; USD stands in until it loads.
 */
export const useCurrency = (): string => {
  const { data } = useQuery<{ summary: Summary }>(GET_SUMMARY);
  return data?.summary.currency ?? DEFAULT_CURRENCY;
};
