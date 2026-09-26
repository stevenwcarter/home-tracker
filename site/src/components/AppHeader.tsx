import { Ref } from 'react';
import { Link, NavLink } from 'react-router-dom';
import { NAV_LINKS, navLinkClass } from 'components/navLinks';
import { SearchBox } from 'components/SearchBox';
import { useTheme } from 'theme/useTheme';

interface AppHeaderProps {
  /** Whether the (below-`md`) locations drawer is open. */
  drawerOpen: boolean;
  onOpenDrawer: () => void;
  /** The hamburger button, so closing the drawer can hand focus back to it. */
  hamburgerRef?: Ref<HTMLButtonElement>;
}

export const AppHeader = ({ drawerOpen, onOpenDrawer, hamburgerRef }: AppHeaderProps) => {
  const { theme, toggle } = useTheme();
  return (
    <header className="border-b border-border bg-surface">
      <div className="flex h-14 items-center gap-2 px-4 md:gap-3">
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
        <Link
          to="/"
          className="shrink-0 text-base font-semibold text-text hover:text-accent md:text-lg"
        >
          Home Tracker
        </Link>
        <SearchBox />
        {/* Below `md` these links live in the drawer, leaving the search box room. */}
        <nav aria-label="Main" className="ml-auto hidden shrink-0 gap-1 md:flex">
          {NAV_LINKS.map(({ to, label }) => (
            <NavLink key={to} to={to} className={navLinkClass}>
              {label}
            </NavLink>
          ))}
        </nav>
        <button
          type="button"
          onClick={toggle}
          aria-label={`Switch to ${theme === 'dark' ? 'light' : 'dark'} theme`}
          className="shrink-0 rounded-md border border-border px-2 py-1 text-xs text-muted hover:text-text md:px-3 md:text-sm"
        >
          {theme === 'dark' ? 'Light' : 'Dark'}
        </button>
      </div>
    </header>
  );
};
