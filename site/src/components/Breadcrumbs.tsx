import { Fragment } from 'react';
import { Link } from 'react-router-dom';
import { LocationSummary } from 'types/entity';

interface BreadcrumbsProps {
  /** Root-first ancestors, each rendered as a link. */
  trail: LocationSummary[];
  /** The current page's name, rendered as plain text after the trail. */
  current?: string;
}

const Separator = () => (
  <li aria-hidden="true" className="text-muted">
    /
  </li>
);

export const Breadcrumbs = ({ trail, current }: BreadcrumbsProps) => (
  <nav aria-label="Breadcrumb">
    <ol className="flex flex-wrap items-center gap-2 text-sm">
      <li>
        <Link to="/" className="text-accent hover:underline">
          Home
        </Link>
      </li>
      {trail.map((location) => (
        <Fragment key={location.id}>
          <Separator />
          <li>
            <Link to={`/locations/${location.id}`} className="text-accent hover:underline">
              {location.name}
            </Link>
          </li>
        </Fragment>
      ))}
      {current !== undefined && (
        <>
          <Separator />
          <li aria-current="page" className="text-text">
            {current}
          </li>
        </>
      )}
    </ol>
  </nav>
);
