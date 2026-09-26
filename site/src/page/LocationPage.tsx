import { Link, Navigate, useParams } from 'react-router-dom';
import { Breadcrumbs } from 'components/Breadcrumbs';
import { DANGER_ACTION, PRIMARY_ACTION, SECONDARY_ACTION } from 'components/buttonStyles';
import { EntityList } from 'components/EntityList';
import { LocationCards } from 'components/LocationCards';
import { PageSkeleton } from 'components/PageSkeleton';
import { PhotoGallery } from 'components/PhotoGallery';
import { PhotoUploader } from 'components/PhotoUploader';
import { Section } from 'components/Section';
import { useCurrency } from 'hooks/useCurrency';
import { useEntity } from 'hooks/useEntity';
import { useEntityTypes } from 'hooks/useEntityTypes';
import { NotFound } from 'page/NotFound';
import { EntityDetail } from 'types/entity';
import { locationType } from 'types/builtIns';
import { isGalleryPhoto } from 'utils/photos';
import { plural } from 'utils/plural';
import { useEntityDeletion } from './useEntityDeletion';

/** Why the location cannot be deleted (the server refuses a non-empty one), or null when it can. */
const deleteBlocker = ({ childLocations, items }: EntityDetail): string | null => {
  const held = [
    childLocations.length > 0 && plural(childLocations.length, 'location', 'locations'),
    items.length > 0 && plural(items.length, 'item', 'items'),
  ].filter(Boolean);
  return held.length > 0 ? `Holds ${held.join(' and ')}; move them first` : null;
};

export const LocationPage = () => {
  const { id = '' } = useParams();
  const { entity, loading, notFound } = useEntity(id);
  const currency = useCurrency();
  const { entityTypes } = useEntityTypes();
  const { askToDelete, deleting, dialog } = useEntityDeletion(entity);

  // Once deleted, the evicted entity refetches as missing: show the skeleton, not "Not found".
  if (deleting && !entity) return <PageSkeleton label="Deleting location" />;
  // A refetch after a write keeps the loaded entity on screen; only a first load shows the skeleton.
  if (loading && !entity) return <PageSkeleton label="Loading location" />;
  if (notFound) return <NotFound what="location" />;
  if (!entity) return <p className="text-danger">Could not load this location.</p>;
  if (!entity.isLocation) return <Navigate to={`/items/${id}`} replace />;

  const blocker = deleteBlocker(entity);
  const newPath = `/locations/${id}/new`;
  const newLocationType = locationType(entityTypes);
  return (
    <section>
      <Breadcrumbs trail={entity.ancestors} current={entity.name} />
      <div className="mt-4 flex flex-wrap items-center justify-between gap-4">
        <h1>{entity.name}</h1>
        <div className="flex flex-wrap gap-2">
          <Link to={newPath} className={PRIMARY_ACTION}>
            Add item
          </Link>
          <Link
            to={newLocationType ? `${newPath}?type=${newLocationType.id}` : newPath}
            className={SECONDARY_ACTION}
          >
            Add location
          </Link>
          <Link to={`/locations/${id}/edit`} className={SECONDARY_ACTION}>
            Edit
          </Link>
          <button
            type="button"
            onClick={askToDelete}
            disabled={blocker !== null}
            title={blocker ?? undefined}
            className={DANGER_ACTION}
          >
            Delete
          </button>
        </div>
      </div>
      {entity.description && (
        <p className="mt-2 whitespace-pre-line text-muted">{entity.description}</p>
      )}
      {entity.childLocations.length > 0 && (
        <Section title="Locations">
          <LocationCards locations={entity.childLocations} />
        </Section>
      )}
      {entity.items.length > 0 && (
        <Section title="Items">
          <EntityList items={entity.items} currency={currency} />
        </Section>
      )}
      {blocker === null && <p className="mt-8 text-muted">Nothing stored here yet.</p>}
      <Section title="Photos">
        <PhotoUploader entity={entity}>
          <PhotoGallery photos={entity.attachments.filter(isGalleryPhoto)} />
        </PhotoUploader>
      </Section>
      {dialog}
    </section>
  );
};

export default LocationPage;
