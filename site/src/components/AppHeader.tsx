import { Ref } from 'react';
import { Link, NavLink } from 'react-router-dom';
import { SearchBox } from 'components/SearchBox';
import { useTheme } from 'theme/useTheme';

interface AppHeaderProps {
  /** Whether the (below-`md`) locations drawer is open. */
  drawerOpen: boolean;
  onOpenDrawer: () => void;
  /** The hamburger button, so closing the drawer can hand focus back to it. */
  hamburgerRef?: Ref<HTMLButtonElement>;
}

const NAV_LINKS = [
  { to: '/types', label: 'Types' },
  { to: '/tags', label: 'Tags' },
];

export const AppHeader = ({ drawerOpen, onOpenDrawer, hamburgerRef }: AppHeaderProps) => {
  const { theme, toggle } = useTheme();
  return (
    <header className="border-b border-border bg-surface">
      <div className="flex h-14 items-center gap-3 px-4">
        <button
          ref={hamburgerRef}
          type="button"
          onClick={onOpenDrawer}
          aria-label="Open locations"
          aria-expanded={drawerOpen}
          className="rounded-md border border-border px-2 py-1 text-muted hover:text-text md:hidden"
        >
          <span aria-hidden="true">☰</span>
        </button>
        <Link to="/" className="shrink-0 text-lg font-semibold text-text hover:text-accent">
          Home Tracker
        </Link>
        <SearchBox />
        <nav aria-label="Main" className="ml-auto flex shrink-0 gap-1">
          {NAV_LINKS.map(({ to, label }) => (
            <NavLink
              key={to}
              to={to}
              className={({ isActive }) =>
                `rounded-md px-2 py-1 text-sm hover:text-text ${isActive ? 'text-accent' : 'text-muted'}`
              }
            >
              {label}
            </NavLink>
          ))}
        </nav>
        <button
          type="button"
          onClick={toggle}
          aria-label={`Switch to ${theme === 'dark' ? 'light' : 'dark'} theme`}
          className="shrink-0 rounded-md border border-border px-3 py-1 text-sm text-muted hover:text-text"
        >
          {theme === 'dark' ? 'Light' : 'Dark'}
        </button>
      </div>
    </header>
  );
};
