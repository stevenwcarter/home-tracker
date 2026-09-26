import { useId, useState } from 'react';
import { useDeleteAttachment, useSetPrimaryPhoto } from 'hooks/useAttachmentMutations';
import { AttachmentRef } from 'types/entity';
import { ROW_ACTION, ROW_DANGER_ACTION } from './buttonStyles';
import { ConfirmDialog } from './ConfirmDialog';
import { Lightbox } from './Lightbox';
import { Thumb } from './Thumb';

/** One gallery tile: the thumbnail (opens the viewer), the Primary badge and the photo's actions. */
const PhotoTile = ({
  photo,
  onOpen,
  onDelete,
  onMakePrimary,
  busy,
}: {
  photo: AttachmentRef;
  onOpen: () => void;
  onDelete: () => void;
  onMakePrimary: () => void;
  busy: boolean;
}) => {
  const titleId = useId();
  return (
    <li aria-labelledby={titleId} className="min-w-0">
      <div className="relative">
        <button
          type="button"
          onClick={onOpen}
          aria-label={`View ${photo.title}`}
          className="block w-full rounded-md focus-visible:outline-2 focus-visible:outline-accent"
        >
          <Thumb attachment={photo} size={300} className="aspect-square w-full" />
        </button>
        {photo.primary && (
          <span className="pointer-events-none absolute top-1 left-1 rounded bg-accent px-1.5 py-0.5 text-xs font-medium text-accent-text">
            Primary
          </span>
        )}
      </div>
      <p id={titleId} className="mt-1 truncate text-xs text-muted" title={photo.title}>
        {photo.title}
      </p>
      <div className="flex flex-wrap gap-1">
        {!photo.primary && (
          <button
            type="button"
            onClick={onMakePrimary}
            disabled={busy}
            aria-label={`Make ${photo.title} the primary photo`}
            className={ROW_ACTION}
          >
            Make primary
          </button>
        )}
        <button
          type="button"
          onClick={onDelete}
          disabled={busy}
          aria-label={`Delete ${photo.title}`}
          className={ROW_DANGER_ACTION}
        >
          Delete
        </button>
      </div>
    </li>
  );
};

/**
 * An entity's photos as a wrapping grid of 300 thumbnails. Each has a Primary
 * badge or a Make primary action, and a Delete that asks first; clicking a
 * thumbnail opens the `Lightbox` at that photo. `photos` must all have
 * thumbnails.
 */
export const PhotoGallery = ({ photos }: { photos: AttachmentRef[] }) => {
  const { remove, loading: removing } = useDeleteAttachment();
  const { setPrimary, loading: settingPrimary } = useSetPrimaryPhoto();
  const [doomed, setDoomed] = useState<AttachmentRef | null>(null);
  const [viewing, setViewing] = useState<number | null>(null);

  const confirmDelete = async () => {
    if (!doomed) return;
    // On failure the hook has toasted; either way the dialog closes.
    await remove(doomed.id);
    setDoomed(null);
  };

  if (photos.length === 0) return <p className="text-sm text-muted">No photos yet.</p>;

  return (
    <>
      <ul
        aria-label="Photos"
        className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6"
      >
        {photos.map((photo, index) => (
          <PhotoTile
            key={photo.id}
            photo={photo}
            onOpen={() => setViewing(index)}
            onDelete={() => setDoomed(photo)}
            onMakePrimary={() => void setPrimary(photo.id)}
            busy={removing || settingPrimary}
          />
        ))}
      </ul>
      {viewing !== null && (
        <Lightbox photos={photos} startIndex={viewing} onClose={() => setViewing(null)} />
      )}
      <ConfirmDialog
        open={doomed !== null}
        title={`Delete ${doomed?.title ?? ''}?`}
        body="This cannot be undone."
        confirmLabel="Delete photo"
        onConfirm={confirmDelete}
        onCancel={() => setDoomed(null)}
        busy={removing}
      />
    </>
  );
};
