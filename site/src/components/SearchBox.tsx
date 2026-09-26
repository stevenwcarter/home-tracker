import { FormEvent, useState } from 'react';
import { useNavigate } from 'react-router-dom';

export const SearchBox = () => {
  const [term, setTerm] = useState('');
  const navigate = useNavigate();

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const query = term.trim();
    if (!query) return;
    navigate(`/search?q=${encodeURIComponent(query)}`);
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
