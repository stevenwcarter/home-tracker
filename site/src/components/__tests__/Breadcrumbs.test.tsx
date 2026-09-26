import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { Breadcrumbs } from '../Breadcrumbs';

describe('Breadcrumbs', () => {
  it('renders Home / ancestors as links and the current name as text', () => {
    render(
      <MemoryRouter>
        <Breadcrumbs
          trail={[{ id: 'house', name: 'House', parentId: null, archived: false }]}
          current="Garage"
        />
      </MemoryRouter>,
    );
    const nav = screen.getByRole('navigation', { name: 'Breadcrumb' });
    expect(nav).toHaveTextContent('Home/House/Garage');
    expect(screen.getByRole('link', { name: 'Home' })).toHaveAttribute('href', '/');
    expect(screen.getByRole('link', { name: 'House' })).toHaveAttribute('href', '/locations/house');
    expect(screen.queryByRole('link', { name: 'Garage' })).not.toBeInTheDocument();
    expect(screen.getByText('Garage')).toHaveAttribute('aria-current', 'page');
  });

  it('renders only the trail when there is no current name', () => {
    render(
      <MemoryRouter>
        <Breadcrumbs trail={[]} />
      </MemoryRouter>,
    );
    expect(screen.getByRole('navigation', { name: 'Breadcrumb' })).toHaveTextContent(/^Home$/);
  });
});
