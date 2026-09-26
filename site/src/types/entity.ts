/** Names an entity's kind (e.g. "Location", "Tool") and whether it can hold other entities. */
export interface EntityTypeRef {
  id: string;
  name: string;
  isLocation: boolean;
}

/** An entity type as the types page and pickers list it. */
export interface EntityTypeDetail extends EntityTypeRef {
  description: string | null;
  icon: string | null;
  /** How many entities have this type. */
  entityCount: number;
}

export interface TagRef {
  id: string;
  name: string;
  color: string | null;
}

/** A tag as the tags page and pickers list it. */
export interface TagDetail extends TagRef {
  description: string | null;
  icon: string | null;
  parentId: string | null;
  /** How many entities carry this tag. */
  entityCount: number;
}

export type AttachmentKind = 'PHOTO' | 'MANUAL' | 'WARRANTY' | 'ATTACHMENT' | 'RECEIPT';

export interface AttachmentRef {
  id: string;
  kind: AttachmentKind;
  primary: boolean;
  title: string;
  mimeType: string;
  url: string;
  /** Null for non-images. */
  thumbnailUrl: string | null;
}

export type FieldKind = 'TEXT' | 'NUMBER' | 'BOOLEAN' | 'TIME';

/** A user-defined custom field on an entity. Only the value matching `kind` is meaningful. */
export interface EntityFieldRef {
  id: string;
  name: string;
  kind: FieldKind;
  textValue: string | null;
  numberValue: number | null;
  booleanValue: boolean;
  timeValue: string | null;
}

/** A location entity, trimmed to what the location tree and breadcrumbs need. */
export interface LocationSummary {
  id: string;
  name: string;
  parentId: string | null;
  archived: boolean;
}

/** An entity as it appears in a list (a location's children, search results, root items). */
export interface EntityListItem {
  id: string;
  name: string;
  assetId: string | null;
  quantity: number;
  purchasePriceCents: number;
  archived: boolean;
  primaryPhoto: { thumbnailUrl: string | null } | null;
  entityType: EntityTypeRef;
}

/** The full `Entity` record for a detail page. */
export interface EntityDetail {
  id: string;
  name: string;
  description: string | null;
  entityType: EntityTypeRef;
  isLocation: boolean;
  parent: LocationSummary | null;
  parentId: string | null;
  ancestors: LocationSummary[];
  childLocations: EntityListItem[];
  items: EntityListItem[];
  archived: boolean;
  assetId: string | null;
  quantity: number;
  insured: boolean;
  serialNumber: string | null;
  modelNumber: string | null;
  manufacturer: string | null;
  notes: string | null;
  lifetimeWarranty: boolean;
  warrantyExpires: string | null;
  warrantyDetails: string | null;
  purchaseDate: string | null;
  purchaseFrom: string | null;
  purchasePriceCents: number;
  soldDate: string | null;
  soldTo: string | null;
  soldPriceCents: number;
  soldNotes: string | null;
  tags: TagRef[];
  attachments: AttachmentRef[];
  primaryPhoto: AttachmentRef | null;
  fields: EntityFieldRef[];
  createdAt: string;
  updatedAt: string;
}

/**
 * Every editable field of an entity, as `createEntity`/`updateEntity` take it.
 * An update replaces every field (an omitted optional resets to its default),
 * so every key is required here: build it from the loaded entity with
 * `toEntityInput` rather than sending only the changed fields. Money is in
 * cents; dates are `YYYY-MM-DD`.
 */
export interface EntityInput {
  name: string;
  description: string | null;
  entityTypeId: string;
  parentId: string | null;
  archived: boolean;
  quantity: number;
  insured: boolean;
  serialNumber: string | null;
  modelNumber: string | null;
  manufacturer: string | null;
  notes: string | null;
  lifetimeWarranty: boolean;
  warrantyExpires: string | null;
  warrantyDetails: string | null;
  purchaseDate: string | null;
  purchaseFrom: string | null;
  purchasePriceCents: number;
  soldDate: string | null;
  soldTo: string | null;
  soldPriceCents: number;
  soldNotes: string | null;
  /** The full tag set; replaces the entity's tags. */
  tagIds: string[];
}

/** Every editable field of an entity type; an update replaces them all. */
export interface EntityTypeInput {
  name: string;
  description: string | null;
  icon: string | null;
  isLocation: boolean;
}

/** Every editable field of a tag; an update replaces them all (a null parent makes it top-level). */
export interface TagInput {
  name: string;
  description: string | null;
  color: string | null;
  icon: string | null;
  parentId: string | null;
}
