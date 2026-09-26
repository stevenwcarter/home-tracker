import React, { Suspense, useEffect, useState } from 'react';
import { ApolloClient, HttpLink, InMemoryCache } from '@apollo/client';
import { ApolloProvider } from '@apollo/client/react';
import { ToastContainer } from 'react-toastify';
import { RouteObject, RouterProvider, createBrowserRouter } from 'react-router-dom';
import 'react-toastify/dist/ReactToastify.css';
import { ThemeProvider } from 'theme/ThemeProvider';

const PageTemplate = React.lazy(() => import('page/PageTemplate'));
const HomePage = React.lazy(() => import('page/HomePage'));
const LocationPage = React.lazy(() => import('page/LocationPage'));
const ItemPage = React.lazy(() => import('page/ItemPage'));
const SearchPage = React.lazy(() => import('page/SearchPage'));
const NotFound = React.lazy(() => import('page/NotFound'));
const NewEntityPage = React.lazy(() => import('page/NewEntityPage'));
const EditEntityPage = React.lazy(() => import('page/EditEntityPage'));
const EntityTypesPage = React.lazy(() => import('page/EntityTypesPage'));
const TagsPage = React.lazy(() => import('page/TagsPage'));

const apolloClient = new ApolloClient({
  cache: new InMemoryCache(),
  link: new HttpLink({ uri: '/graphql', credentials: 'include' }),
});

const routes: RouteObject[] = [
  {
    path: '/',
    element: <PageTemplate />,
    children: [
      { index: true, element: <HomePage /> },
      { path: 'new', element: <NewEntityPage /> },
      { path: 'locations/:id', element: <LocationPage /> },
      { path: 'locations/:id/new', element: <NewEntityPage /> },
      { path: 'locations/:id/edit', element: <EditEntityPage /> },
      { path: 'items/:id', element: <ItemPage /> },
      { path: 'items/:id/edit', element: <EditEntityPage /> },
      { path: 'types', element: <EntityTypesPage /> },
      { path: 'tags', element: <TagsPage /> },
      { path: 'search', element: <SearchPage /> },
      { path: '*', element: <NotFound /> },
    ],
  },
];

const App = () => {
  // Created per mount (not at import) so the router reads the URL current at
  // mount, and disposed on unmount so its history listener doesn't outlive the
  // app. Creating it in the effect (not a lazy `useState`) keeps StrictMode's
  // simulated unmount/remount from leaving a disposed router in state.
  const [router, setRouter] = useState<ReturnType<typeof createBrowserRouter> | null>(null);
  useEffect(() => {
    const created = createBrowserRouter(routes);
    setRouter(created);
    return () => created.dispose();
  }, []);
  return (
    <ThemeProvider>
      <ApolloProvider client={apolloClient}>
        <Suspense fallback={<div className="p-8 text-muted">Loading…</div>}>
          <ToastContainer theme="dark" />
          {router && <RouterProvider router={router} />}
        </Suspense>
      </ApolloProvider>
    </ThemeProvider>
  );
};

export default App;
