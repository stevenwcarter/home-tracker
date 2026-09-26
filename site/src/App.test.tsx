import { render, screen, within } from '@testing-library/react';
import App from './App';

// Warm the lazy chunks so a cold transform doesn't eat findBy's timeout when this
// file runs alone (see chore-tracker's App.test.tsx for the history).
beforeAll(async () => {
  await Promise.all([
    import('page/PageTemplate'),
    import('page/HomePage'),
    import('page/LocationPage'),
    import('page/ItemPage'),
    import('page/SearchPage'),
    import('page/NotFound'),
  ]);
});

afterEach(() => window.history.pushState({}, '', '/'));

describe('App', () => {
  it('renders the shell and the home page', async () => {
    render(<App />);
    expect(await screen.findByRole('link', { name: 'Home Tracker' })).toBeInTheDocument();
    expect(await screen.findByText('Total Items')).toBeInTheDocument();
    expect(
      await within(screen.getByRole('main')).findByRole('region', { name: 'Locations' }),
    ).toBeInTheDocument();
  });

  it('routes /locations/:id to the location page (Not found for an unknown id)', async () => {
    window.history.pushState({}, '', '/locations/unknown');
    render(<App />);
    expect(await screen.findByRole('heading', { level: 1, name: 'Not found' })).toBeInTheDocument();
    expect(screen.getByText("That location doesn't exist.")).toBeInTheDocument();
  });

  it('routes /search to the search page', async () => {
    window.history.pushState({}, '', '/search?q=drill');
    render(<App />);
    expect(
      await screen.findByRole('heading', { level: 1, name: 'Results for "drill"' }),
    ).toBeInTheDocument();
    expect(await screen.findByText('No results')).toBeInTheDocument();
  });

  it('renders Not found for an unknown route', async () => {
    window.history.pushState({}, '', '/no/such/page');
    render(<App />);
    expect(await screen.findByRole('heading', { level: 1, name: 'Not found' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Back to home' })).toHaveAttribute('href', '/');
  });
});
