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
 * itself) is never reachable by walking down from the real roots, so each
 * member of the cycle is promoted to its own root afterwards, in name
 * order — attaching a cycle member under its own cycle partner would just
 * relocate the same arbitrary-nesting problem one level down. A location
 * that merely *hangs off* a cycle member without itself being part of the
 * cycle (e.g. a genuine child of a cyclic location) still nests under that
 * member normally: only the reciprocal, cycle-forming edge is cut, real
 * subtrees are preserved. That keeps every id present exactly once and
 * rules out infinite recursion.
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

  // True when walking up `startId`'s own parent chain reaches `candidateId`.
  // Attaching `candidateId` as a child of `startId` in that case would
  // re-create a cycle (candidate is already startId's ancestor), so the
  // caller should leave it for independent promotion instead of nesting it.
  // `seen` bounds the walk so a cycle elsewhere in the chain can't loop
  // forever.
  const isAncestor = (candidateId: string, startId: string): boolean => {
    const seen = new Set<string>();
    let currentId = startId;
    for (;;) {
      const parentId = byId.get(currentId)?.parentId ?? null;
      if (parentId === null || !byId.has(parentId)) return false;
      if (parentId === candidateId) return true;
      if (seen.has(parentId)) return false;
      seen.add(parentId);
      currentId = parentId;
    }
  };

  const visited = new Set<string>();

  const buildNode = (location: LocationSummary): LocationNode => {
    visited.add(location.id);
    const children = (childrenByParent.get(location.id) ?? [])
      .filter((child) => !visited.has(child.id) && !isAncestor(child.id, location.id))
      .sort(byNameCaseInsensitive)
      .map(buildNode);
    return { location, children };
  };

  const roots = (childrenByParent.get(null) ?? []).sort(byNameCaseInsensitive).map(buildNode);

  // Anything left is unreachable from a real root, i.e. part of a cycle.
  // Walk the full list in name order and promote whichever is still
  // unvisited when we reach it. That's equivalent to repeatedly picking the
  // lowest-named unvisited location: every earlier, lower-named entry has
  // by now either been visited above or handled by this same loop (either
  // promoted itself, or attached as a genuine descendant of a location this
  // loop already promoted) — checked live, right before each `buildNode`
  // call, since promoting one location can visit later ones in this list
  // as a side effect, and re-promoting an already-attached one would
  // duplicate it.
  const extraRoots: LocationNode[] = [];
  for (const location of [...locations].sort(byNameCaseInsensitive)) {
    if (!visited.has(location.id)) extraRoots.push(buildNode(location));
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
