import clsx from 'clsx';
import { Link } from 'react-router-dom';
import { LocationNode } from 'utils/locationTree';

interface LocationTreeProps {
  nodes: LocationNode[];
  /** The location whose page is showing, if any. */
  currentId: string | null;
  /** Ids of the nodes whose children are shown. */
  expanded: ReadonlySet<string>;
  onToggle: (id: string) => void;
  /** Called when a location link is followed (the drawer closes on it). */
  onNavigate?: () => void;
}

export const LocationTree = ({
  nodes,
  currentId,
  expanded,
  onToggle,
  onNavigate,
}: LocationTreeProps) => (
  <ul className="space-y-0.5">
    {nodes.map(({ location, children }) => {
      const hasChildren = children.length > 0;
      const isExpanded = hasChildren && expanded.has(location.id);
      const isCurrent = location.id === currentId;
      return (
        <li key={location.id}>
          <div className="flex items-center gap-1">
            {hasChildren ? (
              <button
                type="button"
                onClick={() => onToggle(location.id)}
                aria-label={`${isExpanded ? 'Collapse' : 'Expand'} ${location.name}`}
                aria-expanded={isExpanded}
                className="flex h-6 w-6 shrink-0 items-center justify-center rounded text-muted hover:text-text"
              >
                <span
                  aria-hidden="true"
                  className={clsx('inline-block transition-transform', isExpanded && 'rotate-90')}
                >
                  ›
                </span>
              </button>
            ) : (
              <span aria-hidden="true" className="h-6 w-6 shrink-0" />
            )}
            <Link
              to={`/locations/${location.id}`}
              onClick={onNavigate}
              aria-current={isCurrent ? 'page' : undefined}
              className={clsx(
                'min-w-0 flex-1 truncate rounded px-2 py-1 text-sm hover:text-accent',
                isCurrent && 'bg-surface-raised font-medium',
                location.archived ? 'text-muted' : 'text-text',
              )}
            >
              {location.name}
              {location.archived && ' (archived)'}
            </Link>
          </div>
          {isExpanded && (
            <div className="ml-3 border-l border-border pl-2">
              <LocationTree
                nodes={children}
                currentId={currentId}
                expanded={expanded}
                onToggle={onToggle}
                onNavigate={onNavigate}
              />
            </div>
          )}
        </li>
      );
    })}
  </ul>
);
