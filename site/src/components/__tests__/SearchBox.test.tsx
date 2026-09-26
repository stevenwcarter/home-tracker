import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';
import { SearchBox } from '../SearchBox';

const LocationProbe = () => {
  const location = useLocation();
  return <div data-testid="location">{location.pathname + location.search}</div>;
};

const renderSearchBox = () =>
  render(
    <MemoryRouter initialEntries={['/']}>
      <SearchBox />
      <Routes>
        <Route path="*" element={<LocationProbe />} />
      </Routes>
    </MemoryRouter>,
  );

describe('SearchBox', () => {
  it('is a search landmark with a labelled input', () => {
    renderSearchBox();
    expect(screen.getByRole('search')).toContainElement(screen.getByLabelText('Search items'));
  });

  it('navigates to /search?q=<term> on Enter', async () => {
    renderSearchBox();
    await userEvent.type(screen.getByLabelText('Search items'), 'drill{Enter}');
    expect(screen.getByTestId('location')).toHaveTextContent('/search?q=drill');
  });

  it('encodes the term', async () => {
    renderSearchBox();
    await userEvent.type(screen.getByLabelText('Search items'), ' red & blue {Enter}');
    expect(screen.getByTestId('location')).toHaveTextContent('/search?q=red%20%26%20blue');
  });

  it('does nothing for whitespace-only input', async () => {
    renderSearchBox();
    await userEvent.type(screen.getByLabelText('Search items'), '   {Enter}');
    expect(screen.getByTestId('location')).toHaveTextContent(/^\/$/);
  });
});
