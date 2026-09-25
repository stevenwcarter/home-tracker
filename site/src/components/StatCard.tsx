interface StatCardProps {
  label: string;
  /** `null` renders a loading placeholder. */
  value: string | null;
}

export const StatCard = ({ label, value }: StatCardProps) => (
  <div className="rounded-xl border border-border bg-surface p-5 shadow-sm">
    <div className="text-sm font-medium text-muted">{label}</div>
    {value === null ? (
      <div
        aria-label={`${label} loading`}
        className="mt-2 h-8 w-24 animate-pulse rounded bg-surface-raised"
      />
    ) : (
      <div className="mt-2 text-3xl font-semibold text-text">{value}</div>
    )}
  </div>
);
