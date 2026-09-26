import { Link } from 'react-router-dom';

interface NotFoundProps {
  /** What was being looked up ("location", "item"); a generic page when omitted. */
  what?: string;
}

export const NotFound = ({ what }: NotFoundProps) => (
  <section>
    <h1>Not found</h1>
    <p className="mt-4 text-muted">
      {what ? `That ${what} doesn't exist.` : "That page doesn't exist."}
    </p>
    <Link to="/" className="mt-4 inline-block text-accent hover:underline">
      Back to home
    </Link>
  </section>
);

export default NotFound;
