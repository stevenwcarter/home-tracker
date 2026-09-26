import { Navigate, useParams } from 'react-router-dom';
import { Breadcrumbs } from 'components/Breadcrumbs';
import { EntityList } from 'components/EntityList';
import { LocationCards } from 'components/LocationCards';
import { PageSkeleton } from 'components/PageSkeleton';
import { Section } from 'components/Section';
import { useCurrency } from 'hooks/useCurrency';
import { useEntity } from 'hooks/useEntity';
import { NotFound } from 'page/NotFound';

export const LocationPage = () => {
  const { id = '' } = useParams();
  const { entity, loading, notFound } = useEntity(id);
  const currency = useCurrency();

  if (loading) return <PageSkeleton label="Loading location" />;
  if (notFound) return <NotFound what="location" />;
  if (!entity) return <p className="text-danger">Could not load this location.</p>;
  if (!entity.isLocation) return <Navigate to={`/items/${id}`} replace />;

  const isEmpty = entity.childLocations.length === 0 && entity.items.length === 0;
  return (
    <section>
      <Breadcrumbs trail={entity.ancestors} current={entity.name} />
      <div className="mt-4 flex flex-wrap items-center justify-between gap-4">
        <h1>{entity.name}</h1>
        <button
          type="button"
          disabled
          title="Coming in phase 4"
          className="rounded-md bg-accent px-4 py-2 text-sm font-medium text-accent-text disabled:cursor-not-allowed disabled:opacity-50"
        >
          Add item
        </button>
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
      {isEmpty && <p className="mt-8 text-muted">Nothing stored here yet.</p>}
    </section>
  );
};

export default LocationPage;
