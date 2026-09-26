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
 * Real data can't contain a cycle (the backend refuses to save one), but
 * this stays well-defined if it ever sees one anyway. A location whose
 * parent chain loops back on itself (including a location parented to
 * itself) is never reachable by walking down from the real roots. For each
 * such unresolved region, `findCycle` identifies exactly the locations that
 * are *on* the cycle, as opposed to merely leading into one, and every
 * member is marked visited together, as one batch, before any of them look
 * for children. That's what makes the result independent of naming: a
 * cycle member is always already visited by the time any *other* member
 * looks for it, so cycle partners never nest under each other regardless of
 * name, while a genuine descendant of a cycle member — never itself marked
 * in that batch, whatever its name — nests normally under its real parent
 * once that parent's children get walked. Every id ends up present exactly
 * once, and termination is guaranteed since each outer iteration visits at
 * least one more location than before.
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

  // Follows `start`'s parent chain until a location repeats, then returns
  // just the repeating segment: the locations actually on the cycle. Any
  // earlier part of the walk (a location that merely leads into the cycle)
  // is left out — and left unvisited — so it can nest normally later, under
  // whichever cycle member turns out to be its real parent.
  const findCycle = (start: LocationSummary): LocationSummary[] => {
    const path: LocationSummary[] = [];
    const positionOf = new Map<string, number>();
    let current = start;
    for (;;) {
      const seenAt = positionOf.get(current.id);
      if (seenAt !== undefined) return path.slice(seenAt);
      positionOf.set(current.id, path.length);
      path.push(current);
      const parent = current.parentId !== null ? byId.get(current.parentId) : undefined;
      // Unreachable in practice: any location whose parent chain runs off
      // the edge like this would already have been visited by the roots
      // walk above (it bottoms out in the `null` bucket, not here). Kept
      // as a safe fallback rather than an assumed invariant.
      if (!parent) return path;
      current = parent;
    }
  };

  // Anything left is on, or leads into, a cycle. Resolve one whole cycle at
  // a time, from an arbitrary still-unvisited starting point: find its
  // members, mark all of them visited together, then walk each member's
  // remaining children as roots of their own, in name order.
  const extraRoots: LocationNode[] = [];
  for (const location of locations) {
    if (visited.has(location.id)) continue;
    const cycle = findCycle(location).sort(byNameCaseInsensitive);
    cycle.forEach((member) => visited.add(member.id));
    extraRoots.push(...cycle.map(buildNode));
  }

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
