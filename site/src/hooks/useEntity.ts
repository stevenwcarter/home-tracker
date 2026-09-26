import { useQuery } from '@apollo/client/react';
import { EntityDetail } from 'types/entity';
import { GET_ENTITY } from './queries';
import { useErrorToast } from './useErrorToast';

interface EntityResponse {
  entity: EntityDetail | null;
}

/** Entity `id`; `skip` runs no query (the entity stays null), for a caller with no id to look up. */
export const useEntity = (id: string, { skip = false }: { skip?: boolean } = {}) => {
  const { data, loading, error } = useQuery<EntityResponse>(GET_ENTITY, {
    variables: { id },
    skip,
  });
  useErrorToast(error, 'Error loading');
  const entity = data?.entity ?? null;
  const notFound = !loading && !error && data !== undefined && data.entity === null;
  return { entity, loading, error, notFound };
};
