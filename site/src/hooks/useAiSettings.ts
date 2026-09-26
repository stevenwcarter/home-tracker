import { useQuery } from '@apollo/client/react';
import { AiSettings } from 'types/ai';
import { GET_AI_SETTINGS } from './queries';
import { useErrorToast } from './useErrorToast';

/** The AI settings, or null until they load. */
export const useAiSettings = () => {
  const { data, loading, error } = useQuery<{ aiSettings: AiSettings }>(GET_AI_SETTINGS);
  useErrorToast(error, 'Error loading the AI settings');
  return { settings: data?.aiSettings ?? null, loading, error };
};
