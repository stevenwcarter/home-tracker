import { Outlet } from 'react-router-dom';
import { AppHeader } from 'components/AppHeader';
import { Sidebar } from 'components/Sidebar';

export const PageTemplate = () => (
  <div className="flex min-h-screen flex-col bg-bg text-text">
    <AppHeader />
    <div className="flex flex-1">
      <Sidebar />
      <main className="flex-1 p-4 md:p-8">
        <Outlet />
      </main>
    </div>
  </div>
);

export default PageTemplate;
