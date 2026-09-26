import { Link } from 'react-router-dom';
import { SECONDARY_ACTION } from 'components/buttonStyles';
import { EntityList } from 'components/EntityList';
import { LocationCards } from 'components/LocationCards';
import { Section } from 'components/Section';
import { StatCard } from 'components/StatCard';
import { useCurrency } from 'hooks/useCurrency';
import { useEntityTypes } from 'hooks/useEntityTypes';
import { useLocations } from 'hooks/useLocations';
import { useRootItems } from 'hooks/useRootItems';
import { useSummary } from 'hooks/useSummary';
import { LOCATION_TYPE_ID, locationType } from 'types/builtIns';
import { formatCents } from 'utils/currency';

export const HomePage = () => {
  const { summary, loading } = useSummary();
  const { tree, loading: locationsLoading } = useLocations();
  const { items: rootItems, loading: rootItemsLoading } = useRootItems();
  const hasError = !loading && !summary;
  const currency = useCurrency();
  const { entityTypes } = useEntityTypes();
  // Before the types load (or if they fail), the built-in Location is the best guess.
  const newLocationTypeId = locationType(entityTypes)?.id ?? LOCATION_TYPE_ID;
  return (
    <section>
      <h1>Home</h1>
      {hasError && <p className="mt-4 text-danger">Could not load statistics.</p>}
      <div className="mt-6 grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <StatCard
          label="Total Value"
          value={summary ? formatCents(summary.totalValueCents, summary.currency) : null}
          error={hasError}
        />
        <StatCard
          label="Total Items"
          value={summary ? String(summary.totalItems) : null}
          error={hasError}
        />
        <StatCard
          label="Total Locations"
          value={summary ? String(summary.totalLocations) : null}
          error={hasError}
        />
        <StatCard
          label="Total Tags"
          value={summary ? String(summary.totalTags) : null}
          error={hasError}
        />
      </div>
      <Section
        title="Locations"
        action={
          <Link to={`/new?type=${newLocationTypeId}`} className={SECONDARY_ACTION}>
            Add location
          </Link>
        }
      >
        {!locationsLoading && tree.length === 0 ? (
          <p className="text-muted">No locations yet.</p>
        ) : (
          <LocationCards locations={tree.map((node) => node.location)} />
        )}
      </Section>
      <Section
        title="Items without a location"
        action={
          <Link to="/new" className={SECONDARY_ACTION}>
            Add item
          </Link>
        }
      >
        {!rootItemsLoading && rootItems.length === 0 ? (
          <p className="text-muted">Every item has a location.</p>
        ) : (
          rootItems.length > 0 && <EntityList items={rootItems} currency={currency} />
        )}
      </Section>
    </section>
  );
};

export default HomePage;
