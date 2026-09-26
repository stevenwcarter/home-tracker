import { ReactNode } from 'react';
import { render } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import { MemoryRouter } from 'react-router-dom';
import { GET_ENTITY_TYPES, GET_LOCATIONS, GET_TAGS } from 'hooks/queries';
import { locationSummary } from './entityFixtures';

// Wire shapes (with `__typename`) for the pickers' and form's lookup queries.

export const entityTypeWire = (id: string, name: string, isLocation: boolean) => ({
  __typename: 'EntityType',
  id,
  name,
  description: null,
  icon: null,
  isLocation,
  entityCount: 0,
});

export const ENTITY_TYPES = [
  entityTypeWire('type-loc', 'Location', true),
  entityTypeWire('type-tool', 'Tool', false),
];

export const tagWire = (id: string, name: string) => ({
  __typename: 'Tag',
  id,
  name,
  description: null,
  color: null,
  icon: null,
  parent: null,
  entityCount: 0,
});

export const TAGS = [tagWire('power', 'Power tools'), tagWire('garden', 'Garden')];

/** House > Garage > Shelf, and Office at the top level. */
export const LOCATIONS = [
  locationSummary({ id: 'house', name: 'House' }),
  locationSummary({ id: 'garage', name: 'Garage', parentId: 'house' }),
  locationSummary({ id: 'shelf', name: 'Shelf', parentId: 'garage' }),
  locationSummary({ id: 'office', name: 'Office' }),
];

export const typesMock = (entityTypes = ENTITY_TYPES): MockedResponse => ({
  request: { query: GET_ENTITY_TYPES },
  result: { data: { entityTypes } },
});

export const tagsMock = (tags = TAGS): MockedResponse => ({
  request: { query: GET_TAGS },
  result: { data: { tags } },
});

export const locationsMock = (locations = LOCATIONS): MockedResponse => ({
  request: { query: GET_LOCATIONS },
  result: { data: { locations } },
});

/** Every lookup query an `EntityForm` runs, answered once each. */
export const lookupMocks = (): MockedResponse[] => [typesMock(), tagsMock(), locationsMock()];

/** Renders `ui` inside a router and a `MockedProvider` with `mocks`. */
export const renderWithApollo = (ui: ReactNode, mocks: MockedResponse[]) =>
  render(
    <MockedProvider mocks={mocks}>
      <MemoryRouter>{ui}</MemoryRouter>
    </MockedProvider>,
  );
