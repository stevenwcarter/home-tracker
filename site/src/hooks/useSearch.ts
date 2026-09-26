import { useQuery } from '@apollo/client/react';
import { EntityListItem } from 'types/entity';
import { SEARCH } from './queries';
import { useErrorToast } from './useErrorToast';

interface SearchResponse {
  search: EntityListItem[];
}

export const useSearch = (query: string) => {
  const trimmed = query.trim();
  const { data, loading, error } = useQuery<SearchResponse>(SEARCH, {
    variables: { query: trimmed },
    skip: trimmed === '',
  });
  useErrorToast(error, 'Search failed');
  return { results: data?.search ?? [], loading, error };
};
