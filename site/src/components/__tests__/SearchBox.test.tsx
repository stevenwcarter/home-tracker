import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Link, MemoryRouter, Route, Routes, useLocation } from 'react-router-dom';
import { SearchBox } from '../SearchBox';

const LocationProbe = () => {
  const location = useLocation();
  return <div data-testid="location">{location.pathname + location.search}</div>;
};

const renderSearchBox = (path = '/') =>
  render(
    <MemoryRouter initialEntries={[path]}>
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

  it('is seeded from ?q on the search page', () => {
    renderSearchBox('/search?q=red%20drill');
    expect(screen.getByLabelText('Search items')).toHaveValue('red drill');
  });

  it('re-seeds when ?q changes from outside the box (back/forward, a link)', async () => {
    render(
      <MemoryRouter initialEntries={['/search?q=drill']}>
        <SearchBox />
        <Link to="/search?q=hammer">hammer</Link>
        <Link to="/">home</Link>
      </MemoryRouter>,
    );
    const input = screen.getByLabelText('Search items');
    expect(input).toHaveValue('drill');
    await userEvent.click(screen.getByRole('link', { name: 'hammer' }));
    expect(input).toHaveValue('hammer');
    await userEvent.click(screen.getByRole('link', { name: 'home' }));
    expect(input).toHaveValue('');
  });
});
