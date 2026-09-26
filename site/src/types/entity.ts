/** Names an entity's kind (e.g. "Location", "Tool") and whether it can hold other entities. */
export interface EntityTypeRef {
  id: string;
  name: string;
  isLocation: boolean;
}

export interface TagRef {
  id: string;
  name: string;
  color: string | null;
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
