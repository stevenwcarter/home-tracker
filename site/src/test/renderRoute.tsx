import { render } from '@testing-library/react';
import { MockedProvider } from '@apollo/client/testing/react';
import type { MockedResponse } from '@apollo/client/testing';
import { MemoryRouter, Navigate, Route, Routes } from 'react-router-dom';
import { CurrentPath } from './CurrentPath';
import { LocationPage } from 'page/LocationPage';
import { ItemPage } from 'page/ItemPage';
import { SearchPage } from 'page/SearchPage';
import { HomePage } from 'page/HomePage';
import { NotFound } from 'page/NotFound';
import { NewEntityPage } from 'page/NewEntityPage';
import { EditEntityPage } from 'page/EditEntityPage';
import { EntityTypesPage } from 'page/EntityTypesPage';
import { TagsPage } from 'page/TagsPage';
import { SettingsPage } from 'page/SettingsPage';
import { IngestPage } from 'page/IngestPage';
import { GET_OPEN_INGEST_BATCHES } from 'hooks/queries';
import { AI_SETTINGS, aiSettingsMock } from './aiFixtures';

/**
 * Answers for the AI entry point every location and item page renders (no
 * key, no unfinished batches), used by any page and for any parent. A test's
 * own mocks come first, so its answer wins.
 */
const ambientMocks = (): MockedResponse[] => [
  {
    ...aiSettingsMock({ ...AI_SETTINGS, hasApiKey: false }),
    maxUsageCount: Number.POSITIVE_INFINITY,
  },
  {
    request: { query: GET_OPEN_INGEST_BATCHES, variables: () => true },
    result: { data: { openIngestBatches: [] } },
    maxUsageCount: Number.POSITIVE_INFINITY,
  },
];

/**
 * Renders the page routes (mirroring `App.tsx`, minus the shell) at `path`, so a
 * page's `<Navigate>` lands on the real target page. The current URL is exposed
 * as `data-testid="current-path"` for redirect assertions.
 */
export const renderRoute = (path: string, mocks: MockedResponse[]) =>
  render(
    <MockedProvider mocks={[...mocks, ...ambientMocks()]}>
      <MemoryRouter initialEntries={[path]}>
        <CurrentPath />
        <Routes>
          <Route path="/" element={<HomePage />} />
          <Route path="/new" element={<NewEntityPage />} />
          <Route path="/locations/:id" element={<LocationPage />} />
          <Route path="/locations/:id/new" element={<NewEntityPage />} />
          <Route path="/locations/:id/edit" element={<EditEntityPage />} />
          <Route path="/items/:id" element={<ItemPage />} />
          <Route path="/items/:id/edit" element={<EditEntityPage />} />
          <Route path="/types" element={<EntityTypesPage />} />
          <Route path="/tags" element={<TagsPage />} />
          <Route path="/settings" element={<Navigate to="/settings/ai" replace />} />
          <Route path="/settings/:tab" element={<SettingsPage />} />
          <Route path="/ingest/:batchId" element={<IngestPage />} />
          <Route path="/search" element={<SearchPage />} />
          <Route path="*" element={<NotFound />} />
        </Routes>
      </MemoryRouter>
    </MockedProvider>,
  );
