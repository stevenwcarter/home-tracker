import { useParams } from 'react-router-dom';
import { Breadcrumbs } from 'components/Breadcrumbs';
import type { BackLink } from 'components/ingest/backLink';
import { IngestCollect } from 'components/ingest/IngestCollect';
import { IngestDone } from 'components/ingest/IngestDone';
import { IngestProgress } from 'components/ingest/IngestProgress';
import { IngestReview } from 'components/ingest/IngestReview';
import { PageSkeleton } from 'components/PageSkeleton';
import { useEntity } from 'hooks/useEntity';
import { useIngestBatch } from 'hooks/useIngestBatch';
import { NotFound } from 'page/NotFound';
import type { IngestBatch } from 'types/ingest';
import { entityPath } from 'utils/entityPath';
import { nextReviewable } from 'utils/ingest';

const TITLE = 'Add items with AI';

/** The screen for where the batch stands. */
const Stage = ({ batch, back }: { batch: IngestBatch; back: BackLink | null }) => {
  switch (batch.status) {
    case 'COLLECTING':
      return <IngestCollect batch={batch} back={back} />;
    case 'PROCESSING':
    case 'REVIEWING':
      return (
        <>
          <IngestProgress batch={batch} currentId={nextReviewable(batch)?.id} />
          <IngestReview batch={batch} />
        </>
      );
    case 'DONE':
      return <IngestDone batch={batch} back={back} />;
    default: {
      // A new backend status must be handled above before this compiles.
      const exhaustive: never = batch.status;
      return exhaustive;
    }
  }
};

/**
 * `/ingest/:batchId`: one AI ingest batch, kept live by its event stream,
 * switching between collecting photos, progress and review, and the summary.
 * The breadcrumb runs through the entity the batch was started from.
 */
export const IngestPage = () => {
  const { batchId = '' } = useParams();
  const { batch, loading, notFound } = useIngestBatch(batchId);
  const parentId = batch?.parentId ?? null;
  const { entity: parent, loading: parentLoading } = useEntity(parentId ?? '', {
    skip: parentId === null,
  });

  if (loading && !batch) return <PageSkeleton label="Loading batch" />;
  if (notFound) return <NotFound what="batch" />;
  if (!batch) return <p className="text-danger">Could not load this batch.</p>;

  // Only once the parent is known: its page's path depends on whether it is a
  // location or an item. A batch with no parent, or one whose parent is gone,
  // leads home.
  const back: BackLink | null =
    parentId !== null && parentLoading && !parent
      ? null
      : parent
        ? { name: parent.name, path: entityPath(parent) }
        : { name: 'Home', path: '/' };

  return (
    <section>
      {back && <Breadcrumbs trail={parent ? [...parent.ancestors, parent] : []} current={TITLE} />}
      <h1 className="mt-4">{TITLE}</h1>
      <Stage batch={batch} back={back} />
    </section>
  );
};

export default IngestPage;
