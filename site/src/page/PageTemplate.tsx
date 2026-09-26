import { useState } from 'react';
import { Outlet, useLocation } from 'react-router-dom';
import { AppHeader } from 'components/AppHeader';
import { Sidebar } from 'components/Sidebar';

export const PageTemplate = () => {
  const [drawerOpen, setDrawerOpen] = useState(false);

  // Any navigation closes the drawer (tree link, breadcrumb, search, back button).
  const { pathname, search } = useLocation();
  const route = pathname + search;
  const [drawerRoute, setDrawerRoute] = useState(route);
  if (route !== drawerRoute) {
    setDrawerRoute(route);
    setDrawerOpen(false);
  }

  return (
    <div className="flex min-h-screen flex-col bg-bg text-text">
      <AppHeader drawerOpen={drawerOpen} onOpenDrawer={() => setDrawerOpen(true)} />
      <div className="flex flex-1">
        <Sidebar drawerOpen={drawerOpen} onClose={() => setDrawerOpen(false)} />
        <main className="min-w-0 flex-1 p-4 md:p-8">
          <Outlet />
        </main>
      </div>
    </div>
  );
};

export default PageTemplate;
