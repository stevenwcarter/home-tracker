import { Link } from 'react-router-dom';
import { PRIMARY_ACTION, SECONDARY_ACTION } from 'components/buttonStyles';
import { useEntity } from 'hooks/useEntity';
import type { IngestBatch } from 'types/ingest';
import { entityPath } from 'utils/entityPath';
import { batchCounts } from 'utils/ingest';
import { plural } from 'utils/plural';
import type { BackLink } from './backLink';
import { useStartBatch } from './useStartBatch';

/**
 * A saved entity, by the name it was saved under (the user may have changed
 * the suggestion's), linked to its page once loaded, since the path depends on
 * whether it is a location; `fallback` shows as plain text until then, or if
 * the entity is gone.
 */
const SavedEntity = ({ id, fallback }: { id: string; fallback: string }) => {
  const { entity } = useEntity(id);
  return entity ? (
    <Link to={entityPath(entity)} className="text-accent hover:underline">
      {entity.name}
    </Link>
  ) : (
    <span className="text-muted">{fallback}</span>
  );
};

/**
 * The finished batch: how many items were saved and skipped, links to the
 * saved entities, "Add more items" (a new batch under the same parent) and a
 * way `back` once the parent is known.
 */
export const IngestDone = ({ batch, back }: { batch: IngestBatch; back: BackLink | null }) => {
  const { start, starting } = useStartBatch();
  const { accepted, skipped } = batchCounts(batch);
  const saved = batch.items.flatMap((item, index) =>
    item.status === 'ACCEPTED' && item.entityId
      ? [{ id: item.entityId, fallback: item.suggestion?.name ?? `Item ${index + 1}` }]
      : [],
  );

  return (
    <div className="mt-6 space-y-6">
      <p className="text-text">
        Saved {plural(accepted, 'item', 'items')}, skipped {skipped}.
      </p>
      {saved.length > 0 && (
        <ul aria-label="Saved items" className="space-y-1">
          {saved.map(({ id, fallback }) => (
            <li key={id}>
              <SavedEntity id={id} fallback={fallback} />
            </li>
          ))}
        </ul>
      )}
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={() => void start(batch.parentId)}
          disabled={starting}
          className={PRIMARY_ACTION}
        >
          Add more items
        </button>
        {back && (
          <Link to={back.path} className={SECONDARY_ACTION}>
            Back to {back.name}
          </Link>
        )}
      </div>
    </div>
  );
};
