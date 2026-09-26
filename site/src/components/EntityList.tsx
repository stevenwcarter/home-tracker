import { Link } from 'react-router-dom';
import { Thumb } from 'components/Thumb';
import { EntityListItem } from 'types/entity';
import { formatCents } from 'utils/currency';

interface EntityListProps {
  items: EntityListItem[];
  currency: string;
}

const entityPath = (entity: EntityListItem) =>
  `/${entity.entityType.isLocation ? 'locations' : 'items'}/${entity.id}`;

/** Rows of entities with a thumbnail; locations link to their location page, items to theirs. */
export const EntityList = ({ items, currency }: EntityListProps) => (
  <ul className="divide-y divide-border rounded-xl border border-border bg-surface">
    {items.map((entity) => (
      <li key={entity.id} className="flex items-center gap-4 p-3">
        <Thumb
          attachment={{ thumbnailUrl: entity.primaryPhoto?.thumbnailUrl ?? null }}
          size={300}
          className="h-14 w-14 shrink-0"
        />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <Link
              to={entityPath(entity)}
              className="truncate font-medium text-text hover:text-accent"
            >
              {entity.name}
            </Link>
            {entity.archived && (
              <span className="rounded-full border border-border px-2 text-xs text-muted">
                Archived
              </span>
            )}
          </div>
          {entity.assetId && <div className="text-sm text-muted">{entity.assetId}</div>}
        </div>
        <div className="shrink-0 text-right text-sm">
          {entity.quantity !== 1 && <div className="text-muted">Qty {entity.quantity}</div>}
          {entity.purchasePriceCents > 0 && (
            <div className="text-text">{formatCents(entity.purchasePriceCents, currency)}</div>
          )}
        </div>
      </li>
    ))}
  </ul>
);
