import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MockedProvider } from '@apollo/client/testing/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import PageTemplate from '../PageTemplate';
import { ThemeProvider } from 'theme/ThemeProvider';
import { GET_LOCATIONS } from 'hooks/queries';

const locations = [
  { __typename: 'Entity', id: 'house', name: 'House', parentId: null, archived: false },
];

const renderPage = () =>
  render(
    <ThemeProvider>
      <MockedProvider
        mocks={[{ request: { query: GET_LOCATIONS }, result: { data: { locations } } }]}
      >
        <MemoryRouter initialEntries={['/']}>
          <Routes>
            <Route path="/" element={<PageTemplate />}>
              <Route index element={<p>home content</p>} />
              <Route path="locations/:id" element={<p>location content</p>} />
            </Route>
          </Routes>
        </MemoryRouter>
      </MockedProvider>
    </ThemeProvider>,
  );

describe('PageTemplate drawer', () => {
  it('opens from the hamburger and closes from the backdrop', async () => {
    renderPage();
    await screen.findByRole('link', { name: 'House' });
    expect(screen.queryByRole('button', { name: 'Close locations' })).not.toBeInTheDocument();

    const hamburger = screen.getByRole('button', { name: 'Open locations' });
    expect(hamburger).toHaveAttribute('aria-expanded', 'false');
    await userEvent.click(hamburger);
    expect(hamburger).toHaveAttribute('aria-expanded', 'true');

    await userEvent.click(screen.getByRole('button', { name: 'Close locations' }));
    expect(screen.queryByRole('button', { name: 'Close locations' })).not.toBeInTheDocument();
    expect(hamburger).toHaveAttribute('aria-expanded', 'false');
  });

  it('closes on Escape and returns focus to the hamburger', async () => {
    renderPage();
    await screen.findByRole('link', { name: 'House' });
    const hamburger = screen.getByRole('button', { name: 'Open locations' });
    await userEvent.click(hamburger);
    screen.getByRole('link', { name: 'House' }).focus();
    expect(hamburger).not.toHaveFocus();

    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('button', { name: 'Close locations' })).not.toBeInTheDocument();
    expect(hamburger).toHaveAttribute('aria-expanded', 'false');
    expect(hamburger).toHaveFocus();
  });

  it('ignores Escape while the drawer is closed', async () => {
    renderPage();
    await screen.findByRole('link', { name: 'House' });
    const search = screen.getByLabelText('Search items');
    search.focus();
    await userEvent.keyboard('{Escape}');
    expect(search).toHaveFocus();
  });

  it('closes the drawer on navigation', async () => {
    renderPage();
    await userEvent.click(screen.getByRole('button', { name: 'Open locations' }));
    await userEvent.click(await screen.findByRole('link', { name: 'House' }));
    expect(screen.getByText('location content')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Close locations' })).not.toBeInTheDocument();
  });

  it('renders the header search box', () => {
    renderPage();
    expect(screen.getByRole('search')).toBeInTheDocument();
    return screen.findByRole('link', { name: 'House' });
  });
});
