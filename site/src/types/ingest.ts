import type { AttachmentKind } from './entity';

// The four unions mirror the backend enums in src/kinds.rs, which GraphQL
// spells in upper case; `types/__tests__/enums.test.ts` holds them to it.

/** Where a batch stands: photos being added, being analysed, being reviewed, or finished. */
export type IngestBatchStatus = 'COLLECTING' | 'PROCESSING' | 'REVIEWING' | 'DONE';

/** Where one future entity stands; `READY` and `FAILED` wait for the user to accept or skip. */
export type IngestItemStatus =
  'COLLECTING' | 'QUEUED' | 'ANALYSING' | 'READY' | 'FAILED' | 'ACCEPTED' | 'SKIPPED';

export type IngestPhotoStatus = 'PENDING' | 'DESCRIBED' | 'FAILED';

/** What the model took a photo for; `OTHER` is stored as an `ATTACHMENT`. */
export type SuggestedKind = 'PHOTO' | 'RECEIPT' | 'WARRANTY' | 'MANUAL' | 'OTHER';

/** A staged photo; its URLs are served exactly as an attachment's are. */
export interface IngestPhoto {
  id: string;
  position: number;
  status: IngestPhotoStatus;
  /** Why the photo could not be described. */
  error: string | null;
  title: string;
  mimeType: string;
  sizeBytes: number;
  url: string;
  /** The 300 px thumbnail; null for non-images. */
  thumbnailUrl: string | null;
  suggestedKind: SuggestedKind | null;
  /** The model's one-line account of the photo. */
  summary: string | null;
  /** The text visible in the photo, transcribed. */
  text: string | null;
}

/**
 * What the model proposes for an item. Every field may be null: the photos
 * did not show it. Money is integer cents; dates are `YYYY-MM-DD`.
 */
export interface IngestSuggestion {
  name: string | null;
  description: string | null;
  manufacturer: string | null;
  modelNumber: string | null;
  serialNumber: string | null;
  quantity: number | null;
  purchaseDate: string | null;
  purchaseFrom: string | null;
  purchasePriceCents: number | null;
  warrantyExpires: string | null;
  lifetimeWarranty: boolean | null;
  warrantyDetails: string | null;
  notes: string | null;
  /** Names of existing tags, spelled as the tag is. */
  tagNames: string[];
  /** `low`, `medium` or `high`. */
  confidence: string | null;
  reasoning: string | null;
}

/** One future entity: its photos, the suggestion once analysed, the entity once accepted. */
export interface IngestItem {
  id: string;
  /** Has gaps; label an item by its index in `IngestBatch.items`. */
  position: number;
  status: IngestItemStatus;
  /** Why the analysis failed. */
  error: string | null;
  suggestion: IngestSuggestion | null;
  entityId: string | null;
  /** Empty once the item is accepted or skipped. */
  photos: IngestPhoto[];
}

/** Photos grouped per item, on their way to becoming entities. */
export interface IngestBatch {
  id: string;
  /** The entity the batch was started from: the default parent of its items. */
  parentId: string | null;
  status: IngestBatchStatus;
  items: IngestItem[];
  createdAt: string;
  updatedAt: string;
}

/** The attachment kind the user chose for one staged photo on accept. */
export interface IngestPhotoKindInput {
  photoId: string;
  kind: AttachmentKind;
}
