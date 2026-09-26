import { ReactNode, useId } from 'react';

/** What a `FormField` hands its control so the label and error attach to it. */
export interface FieldControlProps {
  id: string;
  'aria-describedby': string | undefined;
  'aria-invalid': boolean | undefined;
}

interface FormFieldProps {
  label: string;
  /** Shown under the control and announced as its description; also marks it invalid. */
  error?: string | null;
  className?: string;
  children: (control: FieldControlProps) => ReactNode;
}

/** Shared input look for the form controls. */
export const INPUT_CLASS =
  'w-full rounded-md border border-border bg-bg px-3 py-1.5 text-sm text-text placeholder:text-muted focus:border-accent focus:outline-none aria-[invalid=true]:border-danger';

/** A label above a control, with an inline error linked through `aria-describedby`. */
export const FormField = ({ label, error, className, children }: FormFieldProps) => {
  const id = useId();
  const errorId = `${id}-error`;
  return (
    <div className={className}>
      <label htmlFor={id} className="mb-1 block text-sm font-medium text-muted">
        {label}
      </label>
      {children({
        id,
        'aria-describedby': error ? errorId : undefined,
        'aria-invalid': error ? true : undefined,
      })}
      {error && (
        <p id={errorId} className="mt-1 text-sm text-danger">
          {error}
        </p>
      )}
    </div>
  );
};

/** A checkbox with its label to the right. */
export const CheckboxField = ({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) => (
  <label className="flex items-center gap-2 text-sm text-text">
    <input
      type="checkbox"
      checked={checked}
      onChange={(event) => onChange(event.target.checked)}
      className="h-4 w-4 accent-accent"
    />
    {label}
  </label>
);
