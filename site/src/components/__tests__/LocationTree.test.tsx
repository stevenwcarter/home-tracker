import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { LocationTree } from '../LocationTree';
import { buildLocationTree } from 'utils/locationTree';

const tree = buildLocationTree([
  { id: 'house', name: 'House', parentId: null, archived: false },
  { id: 'garage', name: 'Garage', parentId: 'house', archived: false },
  { id: 'shed', name: 'Shed', parentId: null, archived: true },
]);

const renderTree = (props: { expanded?: Set<string>; currentId?: string | null } = {}) => {
  const onToggle = vi.fn();
  const utils = render(
    <MemoryRouter>
      <LocationTree
        nodes={tree}
        currentId={props.currentId ?? null}
        expanded={props.expanded ?? new Set()}
        onToggle={onToggle}
      />
    </MemoryRouter>,
  );
  return { ...utils, onToggle };
};

describe('LocationTree', () => {
  it('renders the roots as links to their location pages', () => {
    renderTree();
    expect(screen.getByRole('link', { name: 'House' })).toHaveAttribute('href', '/locations/house');
    expect(screen.getByRole('link', { name: /Shed/ })).toHaveAttribute('href', '/locations/shed');
  });

  it('hides a collapsed node’s children and toggles via the chevron', async () => {
    const { onToggle } = renderTree();
    expect(screen.queryByRole('link', { name: 'Garage' })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Expand House' }));
    expect(onToggle).toHaveBeenCalledWith('house');
  });

  it('shows an expanded node’s children with a collapse chevron', async () => {
    const { onToggle } = renderTree({ expanded: new Set(['house']) });
    expect(screen.getByRole('link', { name: 'Garage' })).toHaveAttribute(
      'href',
      '/locations/garage',
    );
    await userEvent.click(screen.getByRole('button', { name: 'Collapse House' }));
    expect(onToggle).toHaveBeenCalledWith('house');
  });

  it('has no chevron for a leaf', () => {
    renderTree();
    expect(screen.queryByRole('button', { name: /Shed/ })).not.toBeInTheDocument();
  });

  it('marks the current node with aria-current and the raised surface', () => {
    renderTree({ expanded: new Set(['house']), currentId: 'garage' });
    const current = screen.getByRole('link', { name: 'Garage' });
    expect(current).toHaveAttribute('aria-current', 'page');
    expect(current).toHaveClass('bg-surface-raised');
    expect(screen.getByRole('link', { name: 'House' })).not.toHaveAttribute('aria-current');
  });

  it('mutes archived locations and suffixes them', () => {
    renderTree();
    const shed = screen.getByRole('link', { name: 'Shed (archived)' });
    expect(shed).toHaveClass('text-muted');
  });
});
