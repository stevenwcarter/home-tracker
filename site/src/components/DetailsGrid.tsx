import { ReactNode } from 'react';

export interface DetailRow {
  label: string;
  value: ReactNode;
}

const isEmpty = (value: ReactNode) =>
  value === null ||
  value === undefined ||
  value === false ||
  (typeof value === 'string' && value.trim() === '');

/** A label/value grid that leaves out every row without a value. */
export const DetailsGrid = ({ rows }: { rows: DetailRow[] }) => {
  const filled = rows.filter((row) => !isEmpty(row.value));
  if (filled.length === 0) return null;
  return (
    <dl className="grid grid-cols-1 gap-x-6 gap-y-3 sm:grid-cols-[max-content_1fr]">
      {filled.map(({ label, value }, index) => (
        // Custom field names aren't unique, so the label alone can't be the key.
        <div key={`${index}-${label}`} className="contents">
          <dt className="text-sm font-medium text-muted">{label}</dt>
          <dd className="whitespace-pre-line text-text">{value}</dd>
        </div>
      ))}
    </dl>
  );
};
