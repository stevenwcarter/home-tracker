import { ReactNode, useId } from 'react';

/** A titled page section, exposed to assistive tech as a region named by its heading. */
export const Section = ({ title, children }: { title: string; children: ReactNode }) => {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="mt-8">
      <h2 id={headingId} className="mb-3 text-lg font-semibold text-text">
        {title}
      </h2>
      {children}
    </section>
  );
};
