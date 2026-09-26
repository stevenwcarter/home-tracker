import { ReactNode, useId } from 'react';

/**
 * A titled page section, exposed to assistive tech as a region named by its
 * heading. `action` (a link or button) sits beside the heading.
 */
export const Section = ({
  title,
  action,
  children,
}: {
  title: string;
  action?: ReactNode;
  children: ReactNode;
}) => {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="mt-8">
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <h2 id={headingId} className="text-lg font-semibold text-text">
          {title}
        </h2>
        {action}
      </div>
      {children}
    </section>
  );
};
