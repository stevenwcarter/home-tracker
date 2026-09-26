import { describe, it, expect } from 'vitest';
import { toEntityInput } from '../entityInput';
import { entityDetail } from 'test/entityFixtures';
import { EntityInput } from 'types/entity';

// Every `EntityInput` key the backend accepts. `updateEntity` resets any
// omitted optional, so the edit path must send them all.
const ALL_KEYS: (keyof EntityInput)[] = [
  'name',
  'description',
  'entityTypeId',
  'parentId',
  'archived',
  'quantity',
  'insured',
  'serialNumber',
  'modelNumber',
  'manufacturer',
  'notes',
  'lifetimeWarranty',
  'warrantyExpires',
  'warrantyDetails',
  'purchaseDate',
  'purchaseFrom',
  'purchasePriceCents',
  'soldDate',
  'soldTo',
  'soldPriceCents',
  'soldNotes',
  'tagIds',
];

describe('toEntityInput', () => {
  it('carries every editable field of the loaded entity, and nothing else', () => {
    const entity = entityDetail({
      id: 'drill',
      name: 'Drill',
      description: 'Cordless',
      parentId: 'garage',
      archived: true,
      quantity: 2,
      insured: true,
      serialNumber: 'SN1',
      modelNumber: 'M1',
      manufacturer: 'Acme',
      notes: 'n',
      lifetimeWarranty: true,
      warrantyExpires: '2030-01-01',
      warrantyDetails: 'wd',
      purchaseDate: '2024-05-06',
      purchaseFrom: 'Store',
      purchasePriceCents: 4999,
      soldDate: '2025-01-01',
      soldTo: 'Bob',
      soldPriceCents: 1000,
      soldNotes: 'sn',
      tags: [
        { id: 't1', name: 'Tools', color: null },
        { id: 't2', name: 'Power', color: '#f00' },
      ],
    });
    const input = toEntityInput(entity);
    expect(Object.keys(input).sort()).toEqual([...ALL_KEYS].sort());
    expect(input).toEqual({
      name: 'Drill',
      description: 'Cordless',
      entityTypeId: 'type-tool',
      parentId: 'garage',
      archived: true,
      quantity: 2,
      insured: true,
      serialNumber: 'SN1',
      modelNumber: 'M1',
      manufacturer: 'Acme',
      notes: 'n',
      lifetimeWarranty: true,
      warrantyExpires: '2030-01-01',
      warrantyDetails: 'wd',
      purchaseDate: '2024-05-06',
      purchaseFrom: 'Store',
      purchasePriceCents: 4999,
      soldDate: '2025-01-01',
      soldTo: 'Bob',
      soldPriceCents: 1000,
      soldNotes: 'sn',
      tagIds: ['t1', 't2'],
    });
  });
});
