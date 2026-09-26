import { Link, useNavigate } from 'react-router-dom';
import { PRIMARY_ACTION, SECONDARY_ACTION } from 'components/buttonStyles';
import { useEntity } from 'hooks/useEntity';
import { useIngestMutations } from 'hooks/useIngestMutations';
import type { IngestBatch } from 'types/ingest';
import { entityPath } from 'utils/entityPath';
import { batchCounts } from 'utils/ingest';
import { plural } from 'utils/plural';
import type { BackLink } from './backLink';

/**
 * A saved entity, by the name it was saved under (the user may have changed
 * the suggestion's); `fallback` shows until the entity loads.
 */
const SavedEntityLink = ({ id, fallback }: { id: string; fallback: string }) => {
  const { entity } = useEntity(id);
  return (
    <Link to={entity ? entityPath(entity) : `/items/${id}`} className="text-accent hover:underline">
      {entity?.name ?? fallback}
    </Link>
  );
};

/**
 * The finished batch: how many items were saved and skipped, links to the
 * saved entities, "Add more items" (a new batch under the same parent) and a
 * way `back`.
 */
export const IngestDone = ({ batch, back }: { batch: IngestBatch; back: BackLink }) => {
  const { createBatch, loading } = useIngestMutations();
  const navigate = useNavigate();
  const { accepted, skipped } = batchCounts(batch);
  const saved = batch.items.flatMap((item, index) =>
    item.status === 'ACCEPTED' && item.entityId
      ? [{ id: item.entityId, fallback: item.suggestion?.name ?? `Item ${index + 1}` }]
      : [],
  );

  const addMore = async () => {
    const next = await createBatch(batch.parentId);
    if (next) navigate(`/ingest/${next.id}`);
  };

  return (
    <div className="mt-6 space-y-6">
      <p className="text-text">
        Saved {plural(accepted, 'item', 'items')}, skipped {skipped}.
      </p>
      {saved.length > 0 && (
        <ul aria-label="Saved items" className="space-y-1">
          {saved.map(({ id, fallback }) => (
            <li key={id}>
              <SavedEntityLink id={id} fallback={fallback} />
            </li>
          ))}
        </ul>
      )}
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={() => void addMore()}
          disabled={loading}
          className={PRIMARY_ACTION}
        >
          Add more items
        </button>
        <Link to={back.path} className={SECONDARY_ACTION}>
          Back to {back.name}
        </Link>
      </div>
    </div>
  );
};
