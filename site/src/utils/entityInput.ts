import { EntityDetail, EntityInput } from 'types/entity';

/**
 * The complete `EntityInput` for a loaded entity. `updateEntity` replaces
 * every field, so an edit starts from this and changes only what the user
 * edited; sending a partial input would reset the rest to their defaults.
 */
export const toEntityInput = (entity: EntityDetail): EntityInput => ({
  name: entity.name,
  description: entity.description,
  entityTypeId: entity.entityType.id,
  parentId: entity.parentId,
  archived: entity.archived,
  quantity: entity.quantity,
  insured: entity.insured,
  serialNumber: entity.serialNumber,
  modelNumber: entity.modelNumber,
  manufacturer: entity.manufacturer,
  notes: entity.notes,
  lifetimeWarranty: entity.lifetimeWarranty,
  warrantyExpires: entity.warrantyExpires,
  warrantyDetails: entity.warrantyDetails,
  purchaseDate: entity.purchaseDate,
  purchaseFrom: entity.purchaseFrom,
  purchasePriceCents: entity.purchasePriceCents,
  soldDate: entity.soldDate,
  soldTo: entity.soldTo,
  soldPriceCents: entity.soldPriceCents,
  soldNotes: entity.soldNotes,
  tagIds: entity.tags.map((tag) => tag.id),
});
