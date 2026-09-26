import { EntityDetail, EntityListItem, LocationSummary } from 'types/entity';

// Apollo's MockedProvider needs `__typename` on every object that carries an id,
// so the fixtures include it even though the hand-written types don't declare it.

const LOCATION_TYPE = {
  __typename: 'EntityType',
  id: 'type-loc',
  name: 'Location',
  isLocation: true,
};
const ITEM_TYPE = { __typename: 'EntityType', id: 'type-tool', name: 'Tool', isLocation: false };

export const locationSummary = (
  overrides: Partial<LocationSummary> & Pick<LocationSummary, 'id' | 'name'>,
): LocationSummary =>
  ({ __typename: 'Entity', parentId: null, archived: false, ...overrides }) as LocationSummary;

export const listItem = (
  overrides: Partial<EntityListItem> & Pick<EntityListItem, 'id' | 'name'>,
  isLocation = false,
): EntityListItem =>
  ({
    __typename: 'Entity',
    assetId: null,
    quantity: 1,
    purchasePriceCents: 0,
    archived: false,
    primaryPhoto: null,
    entityType: isLocation ? LOCATION_TYPE : ITEM_TYPE,
    ...overrides,
  }) as EntityListItem;

export const entityDetail = (
  overrides: Partial<EntityDetail> & Pick<EntityDetail, 'id' | 'name'>,
  isLocation = false,
): EntityDetail =>
  ({
    __typename: 'Entity',
    description: null,
    entityType: isLocation ? LOCATION_TYPE : ITEM_TYPE,
    isLocation,
    parent: null,
    parentId: null,
    ancestors: [],
    childLocations: [],
    items: [],
    archived: false,
    assetId: null,
    quantity: 1,
    insured: false,
    serialNumber: null,
    modelNumber: null,
    manufacturer: null,
    notes: null,
    lifetimeWarranty: false,
    warrantyExpires: null,
    warrantyDetails: null,
    purchaseDate: null,
    purchaseFrom: null,
    purchasePriceCents: 0,
    soldDate: null,
    soldTo: null,
    soldPriceCents: 0,
    soldNotes: null,
    tags: [],
    attachments: [],
    primaryPhoto: null,
    fields: [],
    createdAt: '2026-01-01T12:00:00Z',
    updatedAt: '2026-01-02T12:00:00Z',
    ...overrides,
  }) as EntityDetail;

export const SUMMARY = {
  totalValueCents: 1234567,
  currency: 'USD',
  totalItems: 42,
  totalLocations: 7,
  totalTags: 5,
};
