import { describe, it, expect } from 'vitest';
import { batchCounts, kindOfPhoto, nextReviewable, suggestionToInput } from '../ingest';
import { ingestBatch, ingestItem, ingestPhoto, ingestSuggestion } from 'test/ingestFixtures';
import type { IngestItemStatus } from 'types/ingest';

const TAGS = [
  { id: 'tag-elec', name: 'Electronics', color: null },
  { id: 'tag-office', name: 'Office', color: null },
];
const PLACE = { typeId: 'type-item', parentId: 'garage', tags: TAGS };

describe('suggestionToInput', () => {
  it('maps every field of a full suggestion', () => {
    const suggestion = ingestSuggestion({
      name: 'Logitech MX Master 3S',
      description: 'Wireless mouse, graphite',
      manufacturer: 'Logitech',
      modelNumber: 'MR0077',
      serialNumber: '2214LZ0A1B2C',
      quantity: 2,
      purchaseDate: '2024-03-12',
      purchaseFrom: 'Amazon',
      purchasePriceCents: 9999,
      warrantyExpires: '2026-03-12',
      lifetimeWarranty: true,
      warrantyDetails: '2-year limited',
      notes: 'Receipt shows order #113',
      tagNames: ['electronics', 'OFFICE'],
      confidence: 'high',
      reasoning: 'Model text on the label.',
    });
    expect(suggestionToInput(suggestion, PLACE)).toEqual({
      name: 'Logitech MX Master 3S',
      description: 'Wireless mouse, graphite',
      entityTypeId: 'type-item',
      parentId: 'garage',
      archived: false,
      quantity: 2,
      insured: false,
      serialNumber: '2214LZ0A1B2C',
      modelNumber: 'MR0077',
      manufacturer: 'Logitech',
      notes: 'Receipt shows order #113',
      lifetimeWarranty: true,
      warrantyExpires: '2026-03-12',
      warrantyDetails: '2-year limited',
      purchaseDate: '2024-03-12',
      purchaseFrom: 'Amazon',
      purchasePriceCents: 9999,
      soldDate: null,
      soldTo: null,
      soldPriceCents: 0,
      soldNotes: null,
      tagIds: ['tag-elec', 'tag-office'],
    });
  });

  it('fills the defaults for an empty suggestion: blank name, quantity 1, no price, no tags', () => {
    expect(suggestionToInput(ingestSuggestion(), { ...PLACE, parentId: null })).toMatchObject({
      name: '',
      description: null,
      parentId: null,
      quantity: 1,
      lifetimeWarranty: false,
      purchasePriceCents: 0,
      purchaseDate: null,
      tagIds: [],
    });
  });

  it('treats a missing suggestion (a failed item) as an empty one', () => {
    expect(suggestionToInput(null, PLACE)).toEqual(suggestionToInput(ingestSuggestion(), PLACE));
  });

  it('keeps a suggested quantity of 0 rather than defaulting it', () => {
    expect(suggestionToInput(ingestSuggestion({ quantity: 0 }), PLACE).quantity).toBe(0);
  });

  it('drops tag names that match no tag, and a tag named twice', () => {
    const suggestion = ingestSuggestion({ tagNames: ['Kitchen', 'Office', 'office'] });
    expect(suggestionToInput(suggestion, PLACE).tagIds).toEqual(['tag-office']);
  });
});

describe('kindOfPhoto', () => {
  it.each([
    ['PHOTO', 'PHOTO'],
    ['RECEIPT', 'RECEIPT'],
    ['WARRANTY', 'WARRANTY'],
    ['MANUAL', 'MANUAL'],
    ['OTHER', 'ATTACHMENT'],
  ] as const)('maps a suggested %s to %s', (suggested, kind) => {
    expect(kindOfPhoto(ingestPhoto({ id: 'p', suggestedKind: suggested }))).toBe(kind);
  });

  it('defaults an undescribed photo to PHOTO', () => {
    expect(kindOfPhoto(ingestPhoto({ id: 'p', suggestedKind: null }))).toBe('PHOTO');
  });
});

const items = (...statuses: IngestItemStatus[]) =>
  statuses.map((status, index) => ingestItem({ id: `i${index}`, position: index, status }));

describe('nextReviewable', () => {
  it('is the first item that is ready or failed, in batch order', () => {
    const batch = ingestBatch({
      id: 'b',
      items: items('ACCEPTED', 'SKIPPED', 'ANALYSING', 'FAILED', 'READY'),
    });
    expect(nextReviewable(batch)?.id).toBe('i3');
  });

  it('skips over closed and still-running items to a later ready one', () => {
    const batch = ingestBatch({ id: 'b', items: items('ACCEPTED', 'QUEUED', 'READY') });
    expect(nextReviewable(batch)?.id).toBe('i2');
  });

  it('is null while nothing is ready, and once everything is closed', () => {
    expect(
      nextReviewable(ingestBatch({ id: 'b', items: items('QUEUED', 'ANALYSING') })),
    ).toBeNull();
    expect(
      nextReviewable(ingestBatch({ id: 'b', items: items('ACCEPTED', 'SKIPPED') })),
    ).toBeNull();
    expect(nextReviewable(ingestBatch({ id: 'b' }))).toBeNull();
  });
});

describe('batchCounts', () => {
  it('counts items by where they stand, and photos across items', () => {
    const batch = ingestBatch({
      id: 'b',
      items: [
        ...items('QUEUED', 'ANALYSING', 'READY', 'FAILED', 'ACCEPTED', 'ACCEPTED', 'SKIPPED'),
        ingestItem({
          id: 'with-photos',
          status: 'COLLECTING',
          photos: [ingestPhoto({ id: 'p1' }), ingestPhoto({ id: 'p2' })],
        }),
      ],
    });
    expect(batchCounts(batch)).toEqual({
      items: 8,
      photos: 2,
      pending: 3,
      ready: 1,
      failed: 1,
      accepted: 2,
      skipped: 1,
    });
  });
});
