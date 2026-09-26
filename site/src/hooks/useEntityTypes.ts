import { useQuery } from '@apollo/client/react';
import { EntityTypeDetail } from 'types/entity';
import { GET_ENTITY_TYPES } from './queries';
import { useErrorToast } from './useErrorToast';

interface EntityTypesResponse {
  entityTypes: EntityTypeDetail[];
}

export const useEntityTypes = () => {
  const { data, loading, error } = useQuery<EntityTypesResponse>(GET_ENTITY_TYPES);
  useErrorToast(error, 'Error loading types');
  return { entityTypes: data?.entityTypes ?? [], loading, error };
};
