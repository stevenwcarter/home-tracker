import { describe, it, expect } from 'vitest';
import { LocationSummary } from 'types/entity';
import { buildLocationTree, pathTo } from '../locationTree';

function loc(id: string, name: string, parentId: string | null): LocationSummary {
  return { id, name, parentId, archived: false };
}

describe('buildLocationTree', () => {
  it('nests locations by parentId and sorts siblings case-insensitively', () => {
    const locations = [
      loc('house', 'House', null),
      loc('garage', 'garage', 'house'),
      loc('attic', 'Attic', 'house'),
      loc('shelf', 'Shelf', 'garage'),
    ];

    const tree = buildLocationTree(locations);

    expect(tree).toHaveLength(1);
    expect(tree[0].location.id).toBe('house');
    // "Attic" sorts before "garage" case-insensitively even though "g" < "A" in plain ASCII order.
    expect(tree[0].children.map((n) => n.location.id)).toEqual(['attic', 'garage']);
    const garageNode = tree[0].children.find((n) => n.location.id === 'garage');
    expect(garageNode?.children.map((n) => n.location.id)).toEqual(['shelf']);
  });

  it('makes a root out of a location whose parentId is not in the set', () => {
    const locations = [loc('orphan', 'Orphan', 'missing-parent')];

    const tree = buildLocationTree(locations);

    expect(tree.map((n) => n.location.id)).toEqual(['orphan']);
    expect(tree[0].children).toEqual([]);
  });

  it('makes both members of a two-node cycle roots, with no infinite loop and no duplicates', () => {
    const locations = [loc('a', 'Alpha', 'b'), loc('b', 'Beta', 'a')];

    const tree = buildLocationTree(locations);

    expect(tree).toHaveLength(2);
    expect(tree.map((n) => n.location.id).sort()).toEqual(['a', 'b']);
    // Each id appears exactly once across the whole tree, not nested under the other.
    const allIds = tree.flatMap((n) => [n.location.id, ...n.children.map((c) => c.location.id)]);
    expect(allIds.sort()).toEqual(['a', 'b']);
  });

  it('makes a root out of a self-parented location', () => {
    const locations = [loc('self', 'Self', 'self')];

    const tree = buildLocationTree(locations);

    expect(tree.map((n) => n.location.id)).toEqual(['self']);
    expect(tree[0].children).toEqual([]);
  });

  // descendants_of_cycle_members_stay_nested (Task 4 review, blocking): a
  // location that hangs off a cycle member without itself being part of the
  // cycle must still nest under that member, not get flattened alongside it.
  it('keeps a non-cyclic descendant nested under its cycle-member parent', () => {
    const locations = [loc('a', 'Alpha', 'b'), loc('b', 'Beta', 'a'), loc('d', 'Delta', 'a')];

    const tree = buildLocationTree(locations);

    expect(tree).toHaveLength(2);
    const byId = new Map(tree.map((n) => [n.location.id, n]));
    expect([...byId.keys()].sort()).toEqual(['a', 'b']);
    expect(byId.get('a')?.children.map((n) => n.location.id)).toEqual(['d']);
    expect(byId.get('b')?.children).toEqual([]);
    // Every id present exactly once across the whole tree.
    const allIds = tree.flatMap((n) => [n.location.id, ...n.children.map((c) => c.location.id)]);
    expect(allIds.sort()).toEqual(['a', 'b', 'd']);
  });

  // a_deeper_chain_under_a_cycle_is_preserved (Task 4 review, blocking): a
  // multi-level chain hanging off a cycle member survives intact (A > D > E),
  // alongside both cycle members as their own roots.
  it('preserves a deeper chain hanging off a cycle member', () => {
    const locations = [
      loc('a', 'Alpha', 'b'),
      loc('b', 'Beta', 'a'),
      loc('d', 'Delta', 'a'),
      loc('e', 'Echo', 'd'),
    ];

    const tree = buildLocationTree(locations);

    expect(tree).toHaveLength(2);
    const byId = new Map(tree.map((n) => [n.location.id, n]));
    expect([...byId.keys()].sort()).toEqual(['a', 'b']);
    const aNode = byId.get('a');
    expect(aNode?.children.map((n) => n.location.id)).toEqual(['d']);
    expect(aNode?.children[0].children.map((n) => n.location.id)).toEqual(['e']);
    expect(aNode?.children[0].children[0].children).toEqual([]);
    expect(byId.get('b')?.children).toEqual([]);
    const allIds = [
      ...tree.map((n) => n.location.id),
      ...(aNode?.children.map((n) => n.location.id) ?? []),
      ...(aNode?.children[0].children.map((n) => n.location.id) ?? []),
    ];
    expect(allIds.sort()).toEqual(['a', 'b', 'd', 'e']);
  });

  it('returns [] for an empty input', () => {
    expect(buildLocationTree([])).toEqual([]);
  });
});

describe('pathTo', () => {
  const tree = buildLocationTree([
    loc('house', 'House', null),
    loc('garage', 'Garage', 'house'),
    loc('shelf', 'Shelf', 'garage'),
  ]);

  it('returns the root-first path including the node itself', () => {
    expect(pathTo(tree, 'shelf')).toEqual([
      { id: 'house', name: 'House', parentId: null, archived: false },
      { id: 'garage', name: 'Garage', parentId: 'house', archived: false },
      { id: 'shelf', name: 'Shelf', parentId: 'garage', archived: false },
    ]);
  });

  it('returns [] for an unknown id', () => {
    expect(pathTo(tree, 'nope')).toEqual([]);
  });

  it('returns [] for an empty tree', () => {
    expect(pathTo([], 'shelf')).toEqual([]);
  });
});
