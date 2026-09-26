import { EntityTypeDetail } from './entity';

// The seeded entity types (`src/db.rs`), which the server never deletes.

/** The built-in Location type's id. */
export const LOCATION_TYPE_ID = '00000000-0000-7000-8000-000000000001';

/** The built-in Item type's id. */
export const ITEM_TYPE_ID = '00000000-0000-7000-8000-000000000002';

/**
 * The type "Add location" preselects: the built-in Location, else one named
 * Location, else any location type.
 */
export const locationType = (types: EntityTypeDetail[]) =>
  types.find((type) => type.id === LOCATION_TYPE_ID && type.isLocation) ??
  types.find((type) => type.isLocation && type.name === 'Location') ??
  types.find((type) => type.isLocation);
