import { useId } from 'react';
import { ROW_DANGER_ACTION } from 'components/buttonStyles';
import { PhotoUploader } from 'components/PhotoUploader';
import { Thumb } from 'components/Thumb';
import type { IngestItem } from 'types/ingest';

interface IngestItemGroupProps {
  item: IngestItem;
  /** The item's index in the batch; it is labelled "Item <index + 1>", never by `position`. */
  index: number;
  /** Offers "Remove item"; a batch keeps at least one item. */
  removable: boolean;
  onRemoveItem: (item: IngestItem) => void;
  onRemovePhoto: (photoId: string) => void;
  /** Disables the remove buttons while an ingest write runs. */
  busy: boolean;
}

/**
 * One future entity on the collect screen: its staged photos (300 px, each
 * with a remove button) in a grid that wraps to three across on a phone,
 * inside an uploader that stages new photos on this item.
 */
export const IngestItemGroup = ({
  item,
  index,
  removable,
  onRemoveItem,
  onRemovePhoto,
  busy,
}: IngestItemGroupProps) => {
  const headingId = useId();
  const label = `Item ${index + 1}`;
  return (
    <section aria-labelledby={headingId} className="rounded-lg border border-border bg-surface p-4">
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <h2 id={headingId} className="text-lg font-semibold text-text">
          {label}
        </h2>
        {removable && (
          <button
            type="button"
            onClick={() => onRemoveItem(item)}
            disabled={busy}
            className={ROW_DANGER_ACTION}
          >
            Remove item
          </button>
        )}
      </div>
      {/* Keyed, so an item's upload status never shows under another item. */}
      <PhotoUploader key={item.id} target={{ kind: 'ingestItem', itemId: item.id }}>
        {item.photos.length > 0 ? (
          <ul
            aria-label={`Photos of ${label}`}
            className="grid grid-cols-3 gap-2 sm:grid-cols-4 lg:grid-cols-6"
          >
            {item.photos.map((photo, photoIndex) => (
              <li key={photo.id} className="relative">
                <Thumb attachment={photo} size={300} className="aspect-square w-full" />
                <button
                  type="button"
                  onClick={() => onRemovePhoto(photo.id)}
                  disabled={busy}
                  aria-label={`Remove photo ${photoIndex + 1} of ${label}`}
                  className="absolute right-1 top-1 rounded-md bg-surface px-2 text-sm text-danger hover:bg-surface-raised disabled:opacity-50"
                >
                  ×
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-sm text-muted">No photos yet.</p>
        )}
      </PhotoUploader>
    </section>
  );
};
