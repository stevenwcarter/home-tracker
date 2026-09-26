import { useQuery } from '@apollo/client/react';
import { LocationSummary } from 'types/entity';
import { buildLocationTree, LocationNode } from 'utils/locationTree';
import { GET_LOCATIONS } from './queries';
import { useErrorToast } from './useErrorToast';

interface LocationsResponse {
  locations: LocationSummary[];
}

export const useLocations = () => {
  const { data, loading, error } = useQuery<LocationsResponse>(GET_LOCATIONS);
  useErrorToast(error, 'Error loading locations');
  const locations = data?.locations ?? [];
  const tree: LocationNode[] = buildLocationTree(locations);
  return { locations, tree, loading, error };
};
