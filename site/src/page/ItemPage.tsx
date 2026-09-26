import { useState } from 'react';
import { Link, Navigate, useParams } from 'react-router-dom';
import { Breadcrumbs } from 'components/Breadcrumbs';
import {
  DANGER_ACTION,
  ROW_ACTION,
  ROW_DANGER_ACTION,
  SECONDARY_ACTION,
} from 'components/buttonStyles';
import { ConfirmDialog } from 'components/ConfirmDialog';
import { DetailRow, DetailsGrid } from 'components/DetailsGrid';
import { EntityList } from 'components/EntityList';
import { LocationCards } from 'components/LocationCards';
import { PageSkeleton } from 'components/PageSkeleton';
import { PhotoGallery } from 'components/PhotoGallery';
import { PhotoUploader } from 'components/PhotoUploader';
import { Section } from 'components/Section';
import { TagChips } from 'components/TagChips';
import { Thumb } from 'components/Thumb';
import { useCurrency } from 'hooks/useCurrency';
import { useDeleteAttachment, useSetPrimaryPhoto } from 'hooks/useAttachmentMutations';
import { useEntity } from 'hooks/useEntity';
import { NotFound } from 'page/NotFound';
import { AttachmentRef, EntityDetail, EntityFieldRef } from 'types/entity';
import { formatCents } from 'utils/currency';
import { formatDate, formatDateTime } from 'utils/date';
import { isGalleryPhoto } from 'utils/photos';
import { useEntityDeletion } from './useEntityDeletion';

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
    default: {
      // A new backend kind must be handled above before this compiles.
      const exhaustive: never = field.kind;
      return exhaustive;
    }
  }
};

/** Per-attachment Delete, and Make primary for a photo that isn't already. */
const AttachmentActions = ({
  attachment,
  onDelete,
  onMakePrimary,
  busy,
}: {
  attachment: AttachmentRef;
  onDelete: (attachment: AttachmentRef) => void;
  onMakePrimary: (attachment: AttachmentRef) => void;
  busy: boolean;
}) => (
  <div className="mt-1 flex flex-wrap gap-1">
    {attachment.kind === 'PHOTO' && !attachment.primary && (
      <button
        type="button"
        onClick={() => onMakePrimary(attachment)}
        disabled={busy}
        aria-label={`Make ${attachment.title} the primary photo`}
        className={ROW_ACTION}
      >
        Make primary
      </button>
    )}
    <button
      type="button"
      onClick={() => onDelete(attachment)}
      disabled={busy}
      aria-label={`Delete ${attachment.title}`}
      className={ROW_DANGER_ACTION}
    >
      Delete
    </button>
  </div>
);

export const ItemPage = () => {
  const { id = '' } = useParams();
  const { entity, loading, notFound } = useEntity(id);
  const currency = useCurrency();
  const { askToDelete, deleting, dialog } = useEntityDeletion(entity);
  const { remove: removeAttachment, loading: removingAttachment } = useDeleteAttachment();
  const { setPrimary, loading: settingPrimary } = useSetPrimaryPhoto();
  const [doomed, setDoomed] = useState<AttachmentRef | null>(null);

  const deleteAttachment = async () => {
    if (!doomed) return;
    // On failure the hook has toasted; either way the dialog closes.
    await removeAttachment(doomed.id);
    setDoomed(null);
  };

  // Once deleted, the evicted entity refetches as missing: show the skeleton, not "Not found".
  if (deleting && !entity) return <PageSkeleton label="Deleting item" />;
  // A refetch after a write keeps the loaded entity on screen; only a first load shows the skeleton.
  if (loading && !entity) return <PageSkeleton label="Loading item" />;
  if (notFound) return <NotFound what="item" />;
  if (!entity) return <p className="text-danger">Could not load this item.</p>;
  if (entity.isLocation) return <Navigate to={`/locations/${id}`} replace />;

  const photo = heroPhoto(entity);
  const photos = entity.attachments.filter(isGalleryPhoto);
  // Photos are managed in the gallery; everything else is listed as a file.
  const otherAttachments = entity.attachments.filter((attachment) => !isGalleryPhoto(attachment));
  const attachmentActions = (attachment: AttachmentRef) => (
    <AttachmentActions
      attachment={attachment}
      onDelete={setDoomed}
      onMakePrimary={(target) => void setPrimary(target.id)}
      busy={removingAttachment || settingPrimary}
    />
  );

  return (
    <section>
      <Breadcrumbs trail={entity.ancestors} current={entity.name} />
      <div className="mt-4 flex flex-wrap items-center justify-between gap-4">
        <h1>{entity.name}</h1>
        <div className="flex flex-wrap gap-2">
          <Link to={`/items/${id}/edit`} className={SECONDARY_ACTION}>
            Edit
          </Link>
          <button type="button" onClick={askToDelete} className={DANGER_ACTION}>
            Delete
          </button>
        </div>
      </div>
      <div className="mt-6 grid grid-cols-1 gap-8 lg:grid-cols-2">
        {photo && (
          <figure aria-label="Featured photo">
            <a href={photo.url} className="block">
              <Thumb attachment={photo} size={1200} className="w-full" />
            </a>
          </figure>
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
      <Section title="Photos">
        {/* Keyed: the route reuses this page for another id, so an unkeyed
            uploader would carry its status (or running batch) to that entity. */}
        <PhotoUploader key={entity.id} target={{ kind: 'entity', entity }}>
          <PhotoGallery key={entity.id} photos={photos} />
        </PhotoUploader>
      </Section>
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
                {attachmentActions(attachment)}
              </li>
            ))}
          </ul>
        </Section>
      )}
      <p className="mt-8 text-sm text-muted">
        Created {formatDateTime(entity.createdAt)} · Updated {formatDateTime(entity.updatedAt)}
      </p>
      {dialog}
      <ConfirmDialog
        open={doomed !== null}
        title={`Delete ${doomed?.title ?? ''}?`}
        body="This cannot be undone."
        confirmLabel="Delete attachment"
        onConfirm={deleteAttachment}
        onCancel={() => setDoomed(null)}
        busy={removingAttachment}
      />
    </section>
  );
};

export default ItemPage;
