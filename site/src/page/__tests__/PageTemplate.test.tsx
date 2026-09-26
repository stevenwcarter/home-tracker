import { describe, it, expect } from 'vitest';
import { ReactNode, useState } from 'react';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MockedProvider } from '@apollo/client/testing/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import PageTemplate from '../PageTemplate';
import { ThemeProvider } from 'theme/ThemeProvider';
import { GET_LOCATIONS } from 'hooks/queries';
import { ConfirmDialog } from 'components/ConfirmDialog';

const locations = [
  { __typename: 'Entity', id: 'house', name: 'House', parentId: null, archived: false },
];

const renderPage = (home: ReactNode = <p>home content</p>) =>
  render(
    <ThemeProvider>
      <MockedProvider
        mocks={[{ request: { query: GET_LOCATIONS }, result: { data: { locations } } }]}
      >
        <MemoryRouter initialEntries={['/']}>
          <Routes>
            <Route path="/" element={<PageTemplate />}>
              <Route index element={home} />
              <Route path="locations/:id" element={<p>location content</p>} />
            </Route>
          </Routes>
        </MemoryRouter>
      </MockedProvider>
    </ThemeProvider>,
  );

/** A page with a button that opens a `ConfirmDialog`. */
const DialogPage = () => {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Ask
      </button>
      <ConfirmDialog
        open={open}
        title="Really?"
        body="Sure?"
        confirmLabel="Yes"
        onConfirm={() => setOpen(false)}
        onCancel={() => setOpen(false)}
      />
    </>
  );
};

describe('PageTemplate drawer', () => {
  it('lets an open dialog take Escape without also closing the drawer', async () => {
    renderPage(<DialogPage />);
    await screen.findByRole('link', { name: 'House' });
    const hamburger = screen.getByRole('button', { name: 'Open locations' });
    await userEvent.click(hamburger);
    await userEvent.click(screen.getByRole('button', { name: 'Ask' }));
    expect(screen.getByRole('dialog', { name: 'Really?' })).toBeInTheDocument();

    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(hamburger).toHaveAttribute('aria-expanded', 'true');

    await userEvent.keyboard('{Escape}');
    expect(hamburger).toHaveAttribute('aria-expanded', 'false');
  });

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
