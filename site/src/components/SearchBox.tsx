import { FormEvent, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';

export const SearchBox = () => {
  // Seeded from `?q`, and re-seeded whenever `?q` changes (a new search,
  // back/forward), so the box always shows the results page's query.
  const [searchParams] = useSearchParams();
  const query = searchParams.get('q') ?? '';
  const [term, setTerm] = useState(query);
  const [seededFrom, setSeededFrom] = useState(query);
  if (query !== seededFrom) {
    setSeededFrom(query);
    setTerm(query);
  }
  const navigate = useNavigate();

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const trimmed = term.trim();
    if (!trimmed) return;
    navigate(`/search?q=${encodeURIComponent(trimmed)}`);
  };

  return (
    <form role="search" onSubmit={submit} className="min-w-0 flex-1 md:max-w-md">
      <input
        type="search"
        value={term}
        onChange={(event) => setTerm(event.target.value)}
        aria-label="Search items"
        placeholder="Search items…"
        className="w-full rounded-md border border-border bg-bg px-3 py-1.5 text-sm text-text placeholder:text-muted focus:border-accent focus:outline-none"
      />
    </form>
  );
};
