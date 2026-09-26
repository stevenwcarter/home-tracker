import { useSearchParams } from 'react-router-dom';
import { EntityList } from 'components/EntityList';
import { PageSkeleton } from 'components/PageSkeleton';
import { useCurrency } from 'hooks/useCurrency';
import { useSearch } from 'hooks/useSearch';

export const SearchPage = () => {
  const [searchParams] = useSearchParams();
  const query = (searchParams.get('q') ?? '').trim();
  const { results, loading, error } = useSearch(query);
  const currency = useCurrency();

  if (query === '') {
    return (
      <section>
        <h1>Search</h1>
        <p className="mt-4 text-muted">Type in the search box to find items.</p>
      </section>
    );
  }

  return (
    <section>
      <h1>Results for &quot;{query}&quot;</h1>
      <div className="mt-6">
        {loading ? (
          <PageSkeleton label="Loading results" />
        ) : error ? (
          <p className="text-danger">Search failed.</p>
        ) : results.length === 0 ? (
          <p className="text-muted">No results</p>
        ) : (
          <EntityList items={results} currency={currency} />
        )}
      </div>
    </section>
  );
};

export default SearchPage;
