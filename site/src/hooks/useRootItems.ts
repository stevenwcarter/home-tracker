import { useQuery } from '@apollo/client/react';
import { EntityListItem } from 'types/entity';
import { GET_ROOT_ITEMS } from './queries';
import { useErrorToast } from './useErrorToast';

interface RootItemsResponse {
  rootItems: EntityListItem[];
}

export const useRootItems = () => {
  const { data, loading, error } = useQuery<RootItemsResponse>(GET_ROOT_ITEMS);
  useErrorToast(error, 'Error loading items');
  return { items: data?.rootItems ?? [], loading, error };
};
