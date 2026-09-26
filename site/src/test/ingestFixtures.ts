import type { IngestBatch, IngestItem, IngestPhoto, IngestSuggestion } from 'types/ingest';

// Apollo's MockedProvider needs `__typename` on every object that carries an
// id, so the fixtures include it even though the hand-written types don't.

export const ingestPhoto = (
  overrides: Partial<IngestPhoto> & Pick<IngestPhoto, 'id'>,
): IngestPhoto =>
  ({
    __typename: 'IngestPhoto',
    position: 0,
    status: 'PENDING',
    error: null,
    title: `${overrides.id}.jpg`,
    mimeType: 'image/jpeg',
    sizeBytes: 1024,
    url: `/ingest/photos/${overrides.id}?v=abc`,
    thumbnailUrl: `/ingest/photos/${overrides.id}/thumb/300?v=abc`,
    suggestedKind: null,
    summary: null,
    text: null,
    ...overrides,
  }) as IngestPhoto;

/** A suggestion with every field null (the photos showed nothing) unless overridden. */
export const ingestSuggestion = (overrides: Partial<IngestSuggestion> = {}): IngestSuggestion =>
  ({
    __typename: 'IngestSuggestion',
    name: null,
    description: null,
    manufacturer: null,
    modelNumber: null,
    serialNumber: null,
    quantity: null,
    purchaseDate: null,
    purchaseFrom: null,
    purchasePriceCents: null,
    warrantyExpires: null,
    lifetimeWarranty: null,
    warrantyDetails: null,
    notes: null,
    tagNames: [],
    confidence: null,
    reasoning: null,
    ...overrides,
  }) as IngestSuggestion;

export const ingestItem = (overrides: Partial<IngestItem> & Pick<IngestItem, 'id'>): IngestItem =>
  ({
    __typename: 'IngestItem',
    position: 0,
    status: 'COLLECTING',
    error: null,
    suggestion: null,
    entityId: null,
    photos: [],
    ...overrides,
  }) as IngestItem;

export const ingestBatch = (
  overrides: Partial<IngestBatch> & Pick<IngestBatch, 'id'>,
): IngestBatch =>
  ({
    __typename: 'IngestBatch',
    parentId: 'garage',
    status: 'COLLECTING',
    items: [],
    createdAt: '2026-09-26T10:00:00Z',
    updatedAt: '2026-09-26T10:00:00Z',
    ...overrides,
  }) as IngestBatch;
