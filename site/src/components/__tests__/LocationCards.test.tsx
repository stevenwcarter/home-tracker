import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { LocationCards } from '../LocationCards';

describe('LocationCards', () => {
  it('links each location to its page', () => {
    render(
      <MemoryRouter>
        <LocationCards
          locations={[
            { id: 'garage', name: 'Garage', archived: false },
            { id: 'attic', name: 'Attic', archived: true },
          ]}
        />
      </MemoryRouter>,
    );
    expect(screen.getByRole('link', { name: 'Garage' })).toHaveAttribute(
      'href',
      '/locations/garage',
    );
    expect(screen.getByRole('link', { name: /Attic/ })).toHaveTextContent('Archived');
  });

  it('renders nothing without locations', () => {
    const { container } = render(
      <MemoryRouter>
        <LocationCards locations={[]} />
      </MemoryRouter>,
    );
    expect(container).toBeEmptyDOMElement();
  });
});
