import { LocationSummary } from 'types/entity';

export interface LocationNode {
  location: LocationSummary;
  children: LocationNode[];
}

function byNameCaseInsensitive(a: LocationSummary, b: LocationSummary): number {
  return a.name.toLowerCase().localeCompare(b.name.toLowerCase());
}

/**
 * Nests a flat list of locations into a tree by `parentId`. A location whose
 * `parentId` doesn't match any id in the list becomes a root, same as a
 * genuinely top-level (`parentId: null`) location.
 *
 * Real data can't contain a cycle (the backend refuses to save one), but this
 * stays well-defined if it ever sees one anyway: a location whose parent
 * chain loops back on itself (including a location parented to itself) is
 * never reachable by walking down from the real roots, so every member of
 * the cycle is promoted to its own root afterwards, in name order, rather
 * than nesting one arbitrarily under another. That keeps each id present
 * exactly once and rules out infinite recursion.
 */
export function buildLocationTree(locations: LocationSummary[]): LocationNode[] {
  const byId = new Map(locations.map((location) => [location.id, location]));
  const childrenByParent = new Map<string | null, LocationSummary[]>();
  for (const location of locations) {
    const parentKey =
      location.parentId !== null && byId.has(location.parentId) ? location.parentId : null;
    const siblings = childrenByParent.get(parentKey);
    if (siblings) siblings.push(location);
    else childrenByParent.set(parentKey, [location]);
  }

  const visited = new Set<string>();

  const buildNode = (location: LocationSummary): LocationNode => {
    visited.add(location.id);
    const children = (childrenByParent.get(location.id) ?? [])
      .filter((child) => !visited.has(child.id))
      .sort(byNameCaseInsensitive)
      .map(buildNode);
    return { location, children };
  };

  const roots = (childrenByParent.get(null) ?? []).sort(byNameCaseInsensitive).map(buildNode);

  // Anything left is unreachable from a real root, i.e. part of a cycle. Mark
  // every remaining location visited up front (before any of them look for
  // children) so cyclic partners become siblings, not one nesting the other.
  const leftover = locations.filter((location) => !visited.has(location.id));
  leftover.sort(byNameCaseInsensitive);
  leftover.forEach((location) => visited.add(location.id));
  const extraRoots = leftover.map((location) => ({ location, children: [] as LocationNode[] }));

  return [...roots, ...extraRoots];
}

/** Root-first path to `id`, including `id` itself; `[]` when `id` isn't in `tree`. */
export function pathTo(tree: LocationNode[], id: string): LocationSummary[] {
  for (const node of tree) {
    if (node.location.id === id) return [node.location];
    const pathFromChild = pathTo(node.children, id);
    if (pathFromChild.length > 0) return [node.location, ...pathFromChild];
  }
  return [];
}
