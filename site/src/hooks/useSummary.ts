import { useQuery } from '@apollo/client/react';
import { Summary } from 'types/summary';
import { GET_SUMMARY } from './queries';
import { useErrorToast } from './useErrorToast';

interface SummaryResponse {
  summary: Summary;
}

export const useSummary = () => {
  const { data, loading, error } = useQuery<SummaryResponse>(GET_SUMMARY);
  useErrorToast(error, 'Error loading summary');
  return { summary: data?.summary ?? null, loading };
};
