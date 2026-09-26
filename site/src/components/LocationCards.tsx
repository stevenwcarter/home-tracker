import { Link } from 'react-router-dom';

interface LocationCardProps {
  id: string;
  name: string;
  archived: boolean;
}

/** A grid of cards, one per location, each linking to its location page. */
export const LocationCards = ({ locations }: { locations: LocationCardProps[] }) =>
  locations.length === 0 ? null : (
    <ul className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3">
      {locations.map((location) => (
        <li key={location.id}>
          <Link
            to={`/locations/${location.id}`}
            className="flex items-center justify-between gap-2 rounded-xl border border-border bg-surface p-4 font-medium text-text shadow-sm hover:border-accent hover:text-accent"
          >
            <span className="truncate">{location.name}</span>
            {location.archived && (
              <span className="shrink-0 text-xs font-normal text-muted">Archived</span>
            )}
          </Link>
        </li>
      ))}
    </ul>
  );
