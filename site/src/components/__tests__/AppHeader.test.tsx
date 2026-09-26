import { describe, it, expect } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import { MemoryRouter } from 'react-router-dom';
import { AppHeader } from '../AppHeader';
import { ThemeProvider } from 'theme/ThemeProvider';

describe('AppHeader', () => {
  it('links Settings in the main nav', () => {
    render(
      <ThemeProvider>
        <MockedProvider mocks={[]}>
          <MemoryRouter>
            <AppHeader drawerOpen={false} onOpenDrawer={() => {}} />
          </MemoryRouter>
        </MockedProvider>
      </ThemeProvider>,
    );
    const nav = screen.getByRole('navigation', { name: 'Main' });
    expect(within(nav).getByRole('link', { name: 'Settings' })).toHaveAttribute(
      'href',
      '/settings',
    );
  });
});
