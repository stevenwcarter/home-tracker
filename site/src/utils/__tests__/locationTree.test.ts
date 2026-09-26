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
