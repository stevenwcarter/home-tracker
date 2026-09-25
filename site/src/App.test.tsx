import { render, screen } from '@testing-library/react';
import App from './App';

// Warm the lazy chunks so a cold transform doesn't eat findBy's timeout when this
// file runs alone (see chore-tracker's App.test.tsx for the history).
beforeAll(async () => {
  await Promise.all([import('page/PageTemplate'), import('page/HomePage')]);
});

describe('App', () => {
  it('renders the shell and the home page', async () => {
    render(<App />);
    expect(await screen.findByRole('link', { name: 'Home Tracker' })).toBeInTheDocument();
    expect(await screen.findByText('Total Items')).toBeInTheDocument();
  });
});
