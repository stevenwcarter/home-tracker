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
    import('page/NewEntityPage'),
    import('page/EditEntityPage'),
    import('page/EntityTypesPage'),
    import('page/TagsPage'),
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

  it('links Types and Tags from the header', async () => {
    render(<App />);
    const nav = await screen.findByRole('navigation', { name: 'Main' });
    expect(within(nav).getByRole('link', { name: 'Types' })).toHaveAttribute('href', '/types');
    expect(within(nav).getByRole('link', { name: 'Tags' })).toHaveAttribute('href', '/tags');
  });

  it.each([
    ['/types', 'Types'],
    ['/tags', 'Tags'],
    ['/new', 'New item'],
    ['/locations/garage/new', 'New item'],
  ])('routes %s', async (path, heading) => {
    window.history.pushState({}, '', path);
    render(<App />);
    expect(await screen.findByRole('heading', { level: 1, name: heading })).toBeInTheDocument();
  });

  it.each(['/items/unknown/edit', '/locations/unknown/edit'])(
    'routes %s to the edit page (Not found for an unknown id)',
    async (path) => {
      window.history.pushState({}, '', path);
      render(<App />);
      expect(
        await screen.findByRole('heading', { level: 1, name: 'Not found' }),
      ).toBeInTheDocument();
    },
  );
});
