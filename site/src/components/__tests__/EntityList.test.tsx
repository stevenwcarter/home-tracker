import { describe, it, expect } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { EntityList } from '../EntityList';
import { listItem } from 'test/entityFixtures';

const renderList = (items: Parameters<typeof EntityList>[0]['items']) =>
  render(
    <MemoryRouter>
      <EntityList items={items} currency="USD" />
    </MemoryRouter>,
  );

describe('EntityList', () => {
  it('links items to their item page and shows the asset id and a thumbnail', () => {
    const { container } = renderList([
      listItem({
        id: 'drill',
        name: 'Drill',
        assetId: '000-001',
        primaryPhoto: { thumbnailUrl: '/attachments/p1/thumb/500?v=abc' },
      }),
    ]);
    expect(screen.getByRole('link', { name: 'Drill' })).toHaveAttribute('href', '/items/drill');
    expect(screen.getByText('000-001')).toHaveClass('text-muted');
    // Decorative (alt=""): the name is right next to it.
    expect(container.querySelector('img')).toHaveAttribute(
      'src',
      '/attachments/p1/thumb/300?v=abc',
    );
  });

  it('links locations to their location page', () => {
    renderList([listItem({ id: 'garage', name: 'Garage' }, true)]);
    expect(screen.getByRole('link', { name: 'Garage' })).toHaveAttribute(
      'href',
      '/locations/garage',
    );
  });

  it('shows the quantity only when it is not 1', () => {
    renderList([
      listItem({ id: 'screws', name: 'Screws', quantity: 3 }),
      listItem({ id: 'saw', name: 'Saw', quantity: 1 }),
    ]);
    const [screws, saw] = screen.getAllByRole('listitem');
    expect(within(screws).getByText('Qty 3')).toBeInTheDocument();
    expect(within(saw).queryByText(/Qty/)).not.toBeInTheDocument();
  });

  it('shows the price only when it is greater than 0', () => {
    renderList([
      listItem({ id: 'drill', name: 'Drill', purchasePriceCents: 4999 }),
      listItem({ id: 'rag', name: 'Rag', purchasePriceCents: 0 }),
    ]);
    const [drill, rag] = screen.getAllByRole('listitem');
    expect(within(drill).getByText('$49.99')).toBeInTheDocument();
    expect(within(rag).queryByText(/\$/)).not.toBeInTheDocument();
  });

  it('marks archived entities', () => {
    renderList([
      listItem({ id: 'old', name: 'Old', archived: true }),
      listItem({ id: 'new', name: 'New' }),
    ]);
    const [old, current] = screen.getAllByRole('listitem');
    expect(within(old).getByText('Archived')).toBeInTheDocument();
    expect(within(current).queryByText('Archived')).not.toBeInTheDocument();
  });
});
