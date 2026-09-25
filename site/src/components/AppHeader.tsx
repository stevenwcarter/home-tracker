import { Link } from 'react-router-dom';
import { useTheme } from 'theme/useTheme';

export const AppHeader = () => {
  const { theme, toggle } = useTheme();
  return (
    <header className="border-b border-border bg-surface">
      <div className="flex h-14 items-center justify-between px-4">
        <Link to="/" className="text-lg font-semibold text-text hover:text-accent">
          Home Tracker
        </Link>
        <button
          type="button"
          onClick={toggle}
          aria-label={`Switch to ${theme === 'dark' ? 'light' : 'dark'} theme`}
          className="rounded-md border border-border px-3 py-1 text-sm text-muted hover:text-text"
        >
          {theme === 'dark' ? 'Light' : 'Dark'}
        </button>
      </div>
    </header>
  );
};
