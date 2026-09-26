/** The app's top-level pages: in the header from `md` up, in the drawer below it. */
export const NAV_LINKS = [
  { to: '/types', label: 'Types' },
  { to: '/tags', label: 'Tags' },
] as const;

/** A nav link's classes, accent-coloured while its page is current. */
export const navLinkClass = ({ isActive }: { isActive: boolean }) =>
  `rounded-md px-2 py-1 text-sm hover:text-text ${isActive ? 'text-accent' : 'text-muted'}`;
