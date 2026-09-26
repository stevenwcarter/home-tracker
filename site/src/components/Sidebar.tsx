import clsx from 'clsx';
import { useEffect, useState } from 'react';
import { LocationTree } from 'components/LocationTree';
import { useCurrentLocationId } from 'hooks/useCurrentLocationId';
import { useLocations } from 'hooks/useLocations';
import { pathTo } from 'utils/locationTree';

/** localStorage key holding the tree's expanded location ids as a JSON array. */
export const EXPANDED_STORAGE_KEY = 'home-tracker.tree.expanded';

/** Reads the stored expanded set, tolerating missing, corrupt, or throwing storage. */
function readStoredExpanded(): Set<string> {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(EXPANDED_STORAGE_KEY) ?? '[]');
    return Array.isArray(parsed)
      ? new Set(parsed.filter((id): id is string => typeof id === 'string'))
      : new Set();
  } catch {
    return new Set();
  }
}

function writeStoredExpanded(expanded: ReadonlySet<string>): void {
  try {
    localStorage.setItem(EXPANDED_STORAGE_KEY, JSON.stringify([...expanded]));
  } catch {
    // Storage is a convenience; the in-memory state still applies.
  }
}

interface SidebarProps {
  /** Below `md` the sidebar is a drawer, shown only while this is true. */
  drawerOpen: boolean;
  onClose: () => void;
}

export const Sidebar = ({ drawerOpen, onClose }: SidebarProps) => {
  const { tree, loading, error } = useLocations();
  const currentId = useCurrentLocationId();

  const [expanded, setExpanded] = useState<Set<string>>(readStoredExpanded);

  // Reveal the current location: expand its ancestors whenever the route (or
  // the loaded tree) yields a new ancestor chain. Adjusting state during
  // render, keyed on the chain, rather than in an effect avoids a flash of the
  // collapsed tree; the user can still collapse an ancestor afterwards.
  const ancestorIds = currentId
    ? pathTo(tree, currentId)
        .slice(0, -1)
        .map((location) => location.id)
    : [];
  const ancestorKey = ancestorIds.join('/');
  const [revealedKey, setRevealedKey] = useState('');
  if (ancestorKey !== revealedKey) {
    setRevealedKey(ancestorKey);
    if (ancestorIds.some((id) => !expanded.has(id))) {
      setExpanded(new Set([...expanded, ...ancestorIds]));
    }
  }

  useEffect(() => writeStoredExpanded(expanded), [expanded]);

  const toggle = (id: string) =>
    setExpanded((previous) => {
      const next = new Set(previous);
      if (!next.delete(id)) next.add(id);
      return next;
    });

  return (
    <>
      {drawerOpen && (
        <button
          type="button"
          aria-label="Close locations"
          onClick={onClose}
          className="fixed inset-0 z-30 bg-bg/70 md:hidden"
        />
      )}
      <aside
        className={clsx(
          'w-64 shrink-0 overflow-y-auto border-r border-border bg-surface p-4',
          drawerOpen ? 'fixed inset-y-0 left-0 z-40 md:static md:z-auto' : 'hidden md:block',
        )}
      >
        <h2 className="text-sm font-semibold uppercase tracking-wide text-muted">Locations</h2>
        <nav aria-label="Locations" className="mt-2">
          {tree.length > 0 ? (
            <LocationTree
              nodes={tree}
              currentId={currentId}
              expanded={expanded}
              onToggle={toggle}
              onNavigate={onClose}
            />
          ) : error ? (
            <p className="text-sm text-danger">Could not load locations.</p>
          ) : (
            <p className="text-sm text-muted">{loading ? 'Loading…' : 'No locations yet.'}</p>
          )}
        </nav>
      </aside>
    </>
  );
};
