import { StatCard } from 'components/StatCard';
import { useSummary } from 'hooks/useSummary';
import { formatCents } from 'utils/currency';

export const HomePage = () => {
  const { summary } = useSummary();
  return (
    <section>
      <h1>Home</h1>
      <div className="mt-6 grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <StatCard
          label="Total Value"
          value={summary ? formatCents(summary.totalValueCents, summary.currency) : null}
        />
        <StatCard label="Total Items" value={summary ? String(summary.totalItems) : null} />
        <StatCard label="Total Locations" value={summary ? String(summary.totalLocations) : null} />
        <StatCard label="Total Tags" value={summary ? String(summary.totalTags) : null} />
      </div>
    </section>
  );
};

export default HomePage;
