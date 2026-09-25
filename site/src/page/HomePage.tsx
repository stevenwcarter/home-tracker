import { StatCard } from 'components/StatCard';
import { useSummary } from 'hooks/useSummary';
import { formatCents } from 'utils/currency';

export const HomePage = () => {
  const { summary, loading } = useSummary();
  const hasError = !loading && !summary;
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
    </section>
  );
};

export default HomePage;
