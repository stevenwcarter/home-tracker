import React, { Suspense, useState } from 'react';
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
      { path: 'locations/:id', element: <LocationPage /> },
      { path: 'items/:id', element: <ItemPage /> },
      { path: 'search', element: <SearchPage /> },
      { path: '*', element: <NotFound /> },
    ],
  },
];

const App = () => {
  // Created per mount (not at import) so the router reads the URL current at render time.
  const [router] = useState(() => createBrowserRouter(routes));
  return (
    <ThemeProvider>
      <ApolloProvider client={apolloClient}>
        <Suspense fallback={<div className="p-8 text-muted">Loading…</div>}>
          <ToastContainer theme="dark" />
          <RouterProvider router={router} />
        </Suspense>
      </ApolloProvider>
    </ThemeProvider>
  );
};

export default App;
