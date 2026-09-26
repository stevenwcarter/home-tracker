import type { AttachmentKind, EntityInput, TagRef } from 'types/entity';
import type {
  IngestBatch,
  IngestItem,
  IngestPhoto,
  IngestSuggestion,
  SuggestedKind,
} from 'types/ingest';

/** Where an accepted item goes, and the tags its suggested names resolve against. */
export interface SuggestionPlacement {
  typeId: string;
  parentId: string | null;
  tags: ReadonlyArray<Pick<TagRef, 'id' | 'name'>>;
}

/**
 * The complete `EntityInput` a suggestion prefills (a failed item has none,
 * and gets the empty form). Money and dates pass through as the server gave
 * them; `quantity` defaults to 1 only when the model found none. Tag names
 * resolve to ids case-insensitively; a name that matches no tag is dropped.
 */
export const suggestionToInput = (
  suggestion: IngestSuggestion | null,
  { typeId, parentId, tags }: SuggestionPlacement,
): EntityInput => {
  const byName = new Map(tags.map((tag) => [tag.name.toLowerCase(), tag.id]));
  const tagIds = new Set<string>();
  for (const name of suggestion?.tagNames ?? []) {
    const id = byName.get(name.toLowerCase());
    if (id) tagIds.add(id);
  }
  return {
    name: suggestion?.name ?? '',
    description: suggestion?.description ?? null,
    entityTypeId: typeId,
    parentId,
    archived: false,
    quantity: suggestion?.quantity ?? 1,
    insured: false,
    serialNumber: suggestion?.serialNumber ?? null,
    modelNumber: suggestion?.modelNumber ?? null,
    manufacturer: suggestion?.manufacturer ?? null,
    notes: suggestion?.notes ?? null,
    lifetimeWarranty: suggestion?.lifetimeWarranty ?? false,
    warrantyExpires: suggestion?.warrantyExpires ?? null,
    warrantyDetails: suggestion?.warrantyDetails ?? null,
    purchaseDate: suggestion?.purchaseDate ?? null,
    purchaseFrom: suggestion?.purchaseFrom ?? null,
    purchasePriceCents: suggestion?.purchasePriceCents ?? 0,
    soldDate: null,
    soldTo: null,
    soldPriceCents: 0,
    soldNotes: null,
    tagIds: [...tagIds],
  };
};

/** The attachment kind each suggested kind is stored as (`From<SuggestedKind>` in src/kinds.rs). */
const ATTACHMENT_KIND: Readonly<Record<SuggestedKind, AttachmentKind>> = {
  PHOTO: 'PHOTO',
  RECEIPT: 'RECEIPT',
  WARRANTY: 'WARRANTY',
  MANUAL: 'MANUAL',
  OTHER: 'ATTACHMENT',
};

/** The kind a staged photo is saved as by default; an undescribed photo is a `PHOTO`. */
export const kindOfPhoto = (photo: Pick<IngestPhoto, 'suggestedKind'>): AttachmentKind =>
  photo.suggestedKind ? ATTACHMENT_KIND[photo.suggestedKind] : 'PHOTO';

const isReviewable = (item: IngestItem): boolean =>
  item.status === 'READY' || item.status === 'FAILED';

/** The item to review next: the first, in batch order, that is `READY` or `FAILED`. */
export const nextReviewable = (batch: Pick<IngestBatch, 'items'>): IngestItem | null =>
  batch.items.find(isReviewable) ?? null;

/**
 * The item to keep reviewing: item `pinnedId` while it is still `READY` or
 * `FAILED`, else (it was accepted, skipped or queued, or is gone) the next
 * reviewable one.
 */
export const pinnedReviewable = (
  batch: Pick<IngestBatch, 'items'>,
  pinnedId: string | null,
): IngestItem | null => {
  const pinned = batch.items.find((item) => item.id === pinnedId);
  return pinned && isReviewable(pinned) ? pinned : nextReviewable(batch);
};

/** How many items stand where; `pending` is every item not yet analysed. */
export interface BatchCounts {
  items: number;
  photos: number;
  pending: number;
  ready: number;
  failed: number;
  accepted: number;
  skipped: number;
}

export const batchCounts = (batch: Pick<IngestBatch, 'items'>): BatchCounts => {
  const counts: BatchCounts = {
    items: batch.items.length,
    photos: 0,
    pending: 0,
    ready: 0,
    failed: 0,
    accepted: 0,
    skipped: 0,
  };
  for (const item of batch.items) {
    counts.photos += item.photos.length;
    switch (item.status) {
      case 'COLLECTING':
      case 'QUEUED':
      case 'ANALYSING':
        counts.pending += 1;
        break;
      case 'READY':
        counts.ready += 1;
        break;
      case 'FAILED':
        counts.failed += 1;
        break;
      case 'ACCEPTED':
        counts.accepted += 1;
        break;
      case 'SKIPPED':
        counts.skipped += 1;
        break;
    }
  }
  return counts;
};
