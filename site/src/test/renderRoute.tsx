import { render } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { CurrentPath } from './CurrentPath';
import { LocationPage } from 'page/LocationPage';
import { ItemPage } from 'page/ItemPage';
import { SearchPage } from 'page/SearchPage';
import { HomePage } from 'page/HomePage';
import { NotFound } from 'page/NotFound';

/**
 * Renders the page routes (mirroring `App.tsx`, minus the shell) at `path`, so a
 * page's `<Navigate>` lands on the real target page. The current URL is exposed
 * as `data-testid="current-path"` for redirect assertions.
 */
export const renderRoute = (path: string, mocks: MockedResponse[]) =>
  render(
    <MockedProvider mocks={mocks}>
      <MemoryRouter initialEntries={[path]}>
        <CurrentPath />
        <Routes>
          <Route path="/" element={<HomePage />} />
          <Route path="/locations/:id" element={<LocationPage />} />
          <Route path="/items/:id" element={<ItemPage />} />
          <Route path="/search" element={<SearchPage />} />
          <Route path="*" element={<NotFound />} />
        </Routes>
      </MemoryRouter>
    </MockedProvider>,
  );
