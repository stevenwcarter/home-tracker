import { useQuery } from '@apollo/client/react';
import { EntityDetail } from 'types/entity';
import { GET_ENTITY } from './queries';
import { useErrorToast } from './useErrorToast';

interface EntityResponse {
  entity: EntityDetail | null;
}

export const useEntity = (id: string) => {
  const { data, loading, error } = useQuery<EntityResponse>(GET_ENTITY, { variables: { id } });
  useErrorToast(error, 'Error loading item');
  const entity = data?.entity ?? null;
  const notFound = !loading && !error && data !== undefined && data.entity === null;
  return { entity, loading, error, notFound };
};
