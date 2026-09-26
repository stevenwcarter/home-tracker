import { useId, useState } from 'react';
import { PRIMARY_ACTION, SECONDARY_ACTION } from 'components/buttonStyles';
import { EntityForm } from 'components/EntityForm';
import { INPUT_CLASS } from 'components/FormField';
import { PageSkeleton } from 'components/PageSkeleton';
import { Thumb } from 'components/Thumb';
import { useEntityTypes } from 'hooks/useEntityTypes';
import { useIngestMutations } from 'hooks/useIngestMutations';
import { useTags } from 'hooks/useTags';
import { itemType } from 'types/builtIns';
import type { AttachmentKind, EntityInput } from 'types/entity';
import type { IngestBatch, IngestItem, IngestPhoto } from 'types/ingest';
import { kindOfPhoto, nextReviewable, suggestionToInput } from 'utils/ingest';

/** The kinds a staged photo can be saved as; `ATTACHMENT` is the model's "other". */
const KIND_OPTIONS: ReadonlyArray<[AttachmentKind, string]> = [
  ['PHOTO', 'Photo'],
  ['RECEIPT', 'Receipt'],
  ['WARRANTY', 'Warranty'],
  ['MANUAL', 'Manual'],
  ['ATTACHMENT', 'Other'],
];

type Kinds = Record<string, AttachmentKind>;

/** One staged photo: its thumbnail, the kind to save it as, and what the model made of it. */
const PhotoCard = ({
  photo,
  index,
  kind,
  onKind,
}: {
  photo: IngestPhoto;
  index: number;
  kind: AttachmentKind;
  onKind: (kind: AttachmentKind) => void;
}) => {
  const selectId = useId();
  return (
    <li className="flex min-w-0 flex-col gap-2">
      <Thumb attachment={photo} size={300} className="aspect-square w-full" />
      <label htmlFor={selectId} className="sr-only">
        Kind of photo {index + 1}
      </label>
      <select
        id={selectId}
        value={kind}
        onChange={(event) => onKind(event.target.value as AttachmentKind)}
        className={INPUT_CLASS}
      >
        {KIND_OPTIONS.map(([value, label]) => (
          <option key={value} value={value}>
            {label}
          </option>
        ))}
      </select>
      {photo.error && <p className="text-sm text-danger">{photo.error}</p>}
      {(photo.summary || photo.text) && (
        <details className="text-sm">
          <summary className="text-muted">What the AI saw</summary>
          {photo.summary && <p className="mt-1 text-text">{photo.summary}</p>}
          {photo.text && (
            <p className="mt-1 whitespace-pre-line break-words text-muted">{photo.text}</p>
          )}
        </details>
      )}
    </li>
  );
};

/** The review of one ready item: its photos with kinds, then the prefilled form. */
const ReadyItem = ({
  item,
  label,
  parentId,
}: {
  item: IngestItem;
  label: string;
  parentId: string | null;
}) => {
  const { acceptItem, skipItem, loading } = useIngestMutations();
  const { entityTypes, loading: typesLoading } = useEntityTypes();
  const { tags, loading: tagsLoading } = useTags();
  const [kinds, setKinds] = useState<Kinds>(() =>
    Object.fromEntries(item.photos.map((photo) => [photo.id, kindOfPhoto(photo)])),
  );
  const suggestion = item.suggestion;
  // The form reads its input once on mount, so the input is fixed the first
  // time both lookups have landed. Later refetches (a tag created from the
  // TagPicker sets `loading` again) must not unmount the form mid-edit.
  const [initialInput, setInitialInput] = useState<EntityInput | null>(null);
  if (initialInput === null && !typesLoading && !tagsLoading) {
    setInitialInput(
      suggestionToInput(suggestion, { typeId: itemType(entityTypes)?.id ?? '', parentId, tags }),
    );
  }
  if (initialInput === null) return <PageSkeleton label="Loading form" />;

  const save = (input: EntityInput) =>
    void acceptItem(
      item.id,
      input,
      item.photos.map((photo) => ({ photoId: photo.id, kind: kinds[photo.id] ?? 'PHOTO' })),
    );

  return (
    <div className="space-y-6">
      <ul
        aria-label={`Photos of ${label}`}
        className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4"
      >
        {item.photos.map((photo, index) => (
          <PhotoCard
            key={photo.id}
            photo={photo}
            index={index}
            kind={kinds[photo.id] ?? kindOfPhoto(photo)}
            onKind={(kind) => setKinds((previous) => ({ ...previous, [photo.id]: kind }))}
          />
        ))}
      </ul>
      {(suggestion?.confidence || suggestion?.reasoning) && (
        <p className="text-sm text-muted">
          {suggestion.confidence && <>Confidence: {suggestion.confidence}. </>}
          {suggestion.reasoning}
        </p>
      )}
      <EntityForm
        mode="create"
        initialInput={initialInput}
        onSubmit={save}
        submitting={loading}
        submitLabel="Save item"
        secondaryAction={
          <button
            type="button"
            onClick={() => void skipItem(item.id)}
            disabled={loading}
            className={SECONDARY_ACTION}
          >
            Skip
          </button>
        }
      />
    </div>
  );
};

/** A failed item: why, with Retry (re-run its failed photos and synthesis) and Skip. */
const FailedItem = ({ item }: { item: IngestItem }) => {
  const { retryItem, skipItem, loading } = useIngestMutations();
  return (
    <div className="space-y-4 rounded-lg border border-border bg-surface p-4">
      <p className="text-danger">{item.error ?? 'The analysis failed.'}</p>
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={() => void retryItem(item.id)}
          disabled={loading}
          className={PRIMARY_ACTION}
        >
          Retry
        </button>
        <button
          type="button"
          onClick={() => void skipItem(item.id)}
          disabled={loading}
          className={SECONDARY_ACTION}
        >
          Skip
        </button>
      </div>
    </div>
  );
};

/**
 * The review pane: the next `READY` or `FAILED` item in batch order, or a
 * waiting note while none is. Saving or skipping one refetches the batch,
 * which moves the pane on to the next; the pane is keyed by item, so kinds and
 * form state never carry over.
 */
export const IngestReview = ({ batch }: { batch: IngestBatch }) => {
  const headingId = useId();
  const item = nextReviewable(batch);
  if (!item) {
    return (
      <p role="status" className="mt-6 text-muted">
        Analysing your photos…
      </p>
    );
  }
  const label = `Item ${batch.items.indexOf(item) + 1}`;
  return (
    <section aria-labelledby={headingId} className="mt-6">
      <h2 id={headingId} className="mb-4 text-lg font-semibold text-text">
        {label}
      </h2>
      {item.status === 'FAILED' ? (
        <FailedItem key={item.id} item={item} />
      ) : (
        <ReadyItem key={item.id} item={item} label={label} parentId={batch.parentId} />
      )}
    </section>
  );
};
