/** The page-level action looks, shared by links and buttons alike. */

/** The main call to action ("Add item"). */
export const PRIMARY_ACTION =
  'rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-accent-text hover:opacity-90 disabled:opacity-50';

/** A secondary action ("Edit", "Cancel"). */
export const SECONDARY_ACTION =
  'rounded-md border border-border px-3 py-1.5 text-sm text-text hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-50';

/** A destructive action that opens a confirm dialog ("Delete"). */
export const DANGER_ACTION =
  'rounded-md border border-border px-3 py-1.5 text-sm text-danger hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-50';

/** A small inline action inside a list or table row. */
export const ROW_ACTION =
  'rounded px-2 py-0.5 text-sm text-muted hover:bg-surface-raised hover:text-text disabled:cursor-not-allowed disabled:opacity-50';

/** `ROW_ACTION` for a destructive row action. */
export const ROW_DANGER_ACTION =
  'rounded px-2 py-0.5 text-sm text-danger hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-50';
