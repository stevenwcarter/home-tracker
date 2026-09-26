/** A pulsing placeholder for a page whose data is still loading. */
export const PageSkeleton = ({ label }: { label: string }) => (
  <div role="status" aria-label={label} className="animate-pulse space-y-4">
    <div className="h-4 w-48 rounded bg-surface-raised" />
    <div className="h-8 w-64 rounded bg-surface-raised" />
    <div className="h-32 rounded-xl bg-surface-raised" />
  </div>
);
