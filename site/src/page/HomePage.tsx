import { EntityList } from 'components/EntityList';
import { LocationCards } from 'components/LocationCards';
import { Section } from 'components/Section';
import { StatCard } from 'components/StatCard';
import { useLocations } from 'hooks/useLocations';
import { useRootItems } from 'hooks/useRootItems';
import { useSummary } from 'hooks/useSummary';
import { formatCents } from 'utils/currency';

const DEFAULT_CURRENCY = 'USD';

export const HomePage = () => {
  const { summary, loading } = useSummary();
  const { tree, loading: locationsLoading } = useLocations();
  const { items: rootItems, loading: rootItemsLoading } = useRootItems();
  const hasError = !loading && !summary;
  const currency = summary?.currency ?? DEFAULT_CURRENCY;
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
      <Section title="Locations">
        {!locationsLoading && tree.length === 0 ? (
          <p className="text-muted">No locations yet.</p>
        ) : (
          <LocationCards locations={tree.map((node) => node.location)} />
        )}
      </Section>
      <Section title="Items without a location">
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
