import { useQuery } from '@apollo/client/react';
import { useLocation } from 'react-router-dom';
import { EntityDetail } from 'types/entity';
import { GET_ENTITY } from './queries';

const LOCATION_PATH = /^\/locations\/([^/]+)/;
const ITEM_PATH = /^\/items\/([^/]+)/;

/**
 * The location the current route is "in": the id on `/locations/:id`, the
 * item's parent on `/items/:id` (read through `GET_ENTITY`, which the item
 * page has already loaded, so no extra request), otherwise null. An item
 * nested in another item yields that item's id, which the location tree
 * does not contain, so nothing is highlighted.
 */
export const useCurrentLocationId = (): string | null => {
  const { pathname } = useLocation();
  const locationId = LOCATION_PATH.exec(pathname)?.[1] ?? null;
  const itemId = ITEM_PATH.exec(pathname)?.[1] ?? null;
  const { data } = useQuery<{ entity: EntityDetail | null }>(GET_ENTITY, {
    variables: { id: itemId ?? '' },
    skip: itemId === null,
  });
  if (locationId !== null) return locationId;
  return itemId === null ? null : (data?.entity?.parentId ?? null);
};
