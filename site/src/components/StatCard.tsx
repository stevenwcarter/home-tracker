interface StatCardProps {
  label: string;
  /** `null` renders a loading placeholder, or an error message when `error` is set. */
  value: string | null;
  /** When `value` is `null`, show an "unavailable" error message instead of a loading placeholder. */
  error?: boolean;
}

export const StatCard = ({ label, value, error = false }: StatCardProps) => (
  <div className="rounded-xl border border-border bg-surface p-5 shadow-sm">
    <div className="text-sm font-medium text-muted">{label}</div>
    {value !== null ? (
      <div className="mt-2 text-3xl font-semibold text-text">{value}</div>
    ) : error ? (
      <div className="mt-2 text-3xl font-semibold text-danger">unavailable</div>
    ) : (
      <div
        aria-label={`${label} loading`}
        className="mt-2 h-8 w-24 animate-pulse rounded bg-surface-raised"
      />
    )}
  </div>
);
