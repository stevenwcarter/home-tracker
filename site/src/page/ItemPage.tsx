import { Navigate, useParams } from 'react-router-dom';
import { Breadcrumbs } from 'components/Breadcrumbs';
import { DetailRow, DetailsGrid } from 'components/DetailsGrid';
import { EntityList } from 'components/EntityList';
import { LocationCards } from 'components/LocationCards';
import { PageSkeleton } from 'components/PageSkeleton';
import { Section } from 'components/Section';
import { TagChips } from 'components/TagChips';
import { Thumb } from 'components/Thumb';
import { useCurrency } from 'hooks/useCurrency';
import { useEntity } from 'hooks/useEntity';
import { NotFound } from 'page/NotFound';
import { AttachmentRef, EntityDetail, EntityFieldRef } from 'types/entity';
import { formatCents } from 'utils/currency';
import { formatDate, formatDateTime } from 'utils/date';

/**
 * Whether an attachment can be shown as a picture. The backend's `is_thumbnailable`
 * is the single MIME gate: it only hands out a `thumbnailUrl` for formats it can
 * decode, so an `image/heic` (say) with no thumbnail is listed as a file instead.
 */
const hasThumbnail = (attachment: AttachmentRef) => attachment.thumbnailUrl !== null;

/** The photo shown large: the primary photo, else the first attachment with a thumbnail. */
const heroPhoto = (entity: EntityDetail): AttachmentRef | null =>
  entity.primaryPhoto ?? entity.attachments.find(hasThumbnail) ?? null;

const price = (cents: number, currency: string) =>
  cents > 0 ? formatCents(cents, currency) : null;

const warranty = (entity: EntityDetail) => {
  if (entity.lifetimeWarranty) return 'Lifetime';
  if (entity.warrantyExpires) return `Expires ${formatDate(entity.warrantyExpires)}`;
  return null;
};

const sold = (entity: EntityDetail, currency: string) => {
  const soldPrice = price(entity.soldPriceCents, currency);
  const parts = [
    entity.soldDate && formatDate(entity.soldDate),
    entity.soldTo && `to ${entity.soldTo}`,
    soldPrice && `for ${soldPrice}`,
  ].filter(Boolean);
  return parts.length > 0 ? parts.join(' ') : null;
};

const detailRows = (entity: EntityDetail, currency: string): DetailRow[] => [
  { label: 'Asset ID', value: entity.assetId },
  { label: 'Type', value: entity.entityType.name },
  { label: 'Quantity', value: String(entity.quantity) },
  { label: 'Manufacturer', value: entity.manufacturer },
  { label: 'Model', value: entity.modelNumber },
  { label: 'Serial number', value: entity.serialNumber },
  { label: 'Purchase date', value: entity.purchaseDate && formatDate(entity.purchaseDate) },
  { label: 'Purchased from', value: entity.purchaseFrom },
  { label: 'Purchase price', value: price(entity.purchasePriceCents, currency) },
  { label: 'Warranty', value: warranty(entity) },
  { label: 'Warranty details', value: entity.warrantyDetails },
  { label: 'Sold', value: sold(entity, currency) },
  { label: 'Sold notes', value: entity.soldNotes },
  { label: 'Insured', value: entity.insured ? 'Yes' : null },
  { label: 'Notes', value: entity.notes },
  { label: 'Description', value: entity.description },
];

const fieldValue = (field: EntityFieldRef): string | null => {
  switch (field.kind) {
    case 'TEXT':
      return field.textValue;
    case 'NUMBER':
      return field.numberValue === null ? null : String(field.numberValue);
    case 'BOOLEAN':
      return field.booleanValue ? 'Yes' : 'No';
    case 'TIME':
      return field.timeValue && formatDateTime(field.timeValue);
  }
};

export const ItemPage = () => {
  const { id = '' } = useParams();
  const { entity, loading, notFound } = useEntity(id);
  const currency = useCurrency();

  if (loading) return <PageSkeleton label="Loading item" />;
  if (notFound) return <NotFound what="item" />;
  if (!entity) return <p className="text-danger">Could not load this item.</p>;
  if (entity.isLocation) return <Navigate to={`/locations/${id}`} replace />;

  const photo = heroPhoto(entity);
  const otherAttachments = entity.attachments.filter((attachment) => attachment.id !== photo?.id);

  return (
    <section>
      <Breadcrumbs trail={entity.ancestors} current={entity.name} />
      <h1 className="mt-4">{entity.name}</h1>
      <div className="mt-6 grid grid-cols-1 gap-8 lg:grid-cols-2">
        {photo && (
          <a href={photo.url} className="block">
            <Thumb attachment={photo} size={1200} className="w-full" />
          </a>
        )}
        <div className="space-y-6">
          <section aria-label="Details">
            <DetailsGrid rows={detailRows(entity, currency)} />
          </section>
          <TagChips tags={entity.tags} />
        </div>
      </div>
      {entity.fields.length > 0 && (
        <Section title="Custom fields">
          <DetailsGrid
            rows={entity.fields.map((field) => ({ label: field.name, value: fieldValue(field) }))}
          />
        </Section>
      )}
      {(entity.childLocations.length > 0 || entity.items.length > 0) && (
        <Section title="Contents">
          <div className="space-y-4">
            {entity.childLocations.length > 0 && (
              <LocationCards locations={entity.childLocations} />
            )}
            {entity.items.length > 0 && <EntityList items={entity.items} currency={currency} />}
          </div>
        </Section>
      )}
      {otherAttachments.length > 0 && (
        <Section title="Attachments">
          <ul className="flex flex-wrap gap-4">
            {otherAttachments.map((attachment) => (
              <li key={attachment.id}>
                <a href={attachment.url} className="text-accent hover:underline">
                  {hasThumbnail(attachment) ? (
                    <Thumb attachment={attachment} size={300} className="h-24 w-24" />
                  ) : (
                    attachment.title
                  )}
                </a>
              </li>
            ))}
          </ul>
        </Section>
      )}
      <p className="mt-8 text-sm text-muted">
        Created {formatDateTime(entity.createdAt)} · Updated {formatDateTime(entity.updatedAt)}
      </p>
    </section>
  );
};

export default ItemPage;
