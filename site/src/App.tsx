import React, { Suspense } from 'react';
import { ApolloClient, HttpLink, InMemoryCache } from '@apollo/client';
import { ApolloProvider } from '@apollo/client/react';
import { ToastContainer } from 'react-toastify';
import { RouterProvider, createBrowserRouter } from 'react-router-dom';
import 'react-toastify/dist/ReactToastify.css';
import { ThemeProvider } from 'theme/ThemeProvider';

const PageTemplate = React.lazy(() => import('page/PageTemplate'));
const HomePage = React.lazy(() => import('page/HomePage'));

const apolloClient = new ApolloClient({
  cache: new InMemoryCache(),
  link: new HttpLink({ uri: '/graphql', credentials: 'include' }),
});

const router = createBrowserRouter([
  {
    path: '/',
    element: <PageTemplate />,
    children: [{ index: true, element: <HomePage /> }],
  },
]);

const App = () => (
  <ThemeProvider>
    <ApolloProvider client={apolloClient}>
      <Suspense fallback={<div className="p-8 text-muted">Loading…</div>}>
        <ToastContainer theme="dark" />
        <RouterProvider router={router} />
      </Suspense>
    </ApolloProvider>
  </ThemeProvider>
);

export default App;
