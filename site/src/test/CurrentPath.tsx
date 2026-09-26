import { useLocation } from 'react-router-dom';

/** Exposes the router's current URL as `data-testid="current-path"` for redirect assertions. */
export const CurrentPath = () => {
  const { pathname, search } = useLocation();
  return <output data-testid="current-path">{pathname + search}</output>;
};
