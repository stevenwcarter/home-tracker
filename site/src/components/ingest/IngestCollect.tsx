import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { DANGER_ACTION, PRIMARY_ACTION, SECONDARY_ACTION } from 'components/buttonStyles';
import { ConfirmDialog } from 'components/ConfirmDialog';
import { useIngestMutations } from 'hooks/useIngestMutations';
import type { IngestBatch, IngestItem } from 'types/ingest';
import { batchCounts } from 'utils/ingest';
import type { BackLink } from './backLink';
import { IngestItemGroup } from './IngestItemGroup';

/**
 * The collecting screen: one group per future entity, "Next item" to start
 * another, "Submit for analysis" once any photo is staged, and "Discard
 * batch" (confirmed) to throw the whole batch away and go `back`.
 */
export const IngestCollect = ({ batch, back }: { batch: IngestBatch; back: BackLink }) => {
  const { addItem, removeItem, removePhoto, submit, deleteBatch, loading } = useIngestMutations();
  const navigate = useNavigate();
  const [doomed, setDoomed] = useState<{ item: IngestItem; label: string } | null>(null);
  const [discarding, setDiscarding] = useState(false);
  const { photos } = batchCounts(batch);

  const askToRemove = (item: IngestItem, label: string) => {
    // An empty item costs nothing to lose; one with photos is confirmed.
    if (item.photos.length === 0) void removeItem(item.id);
    else setDoomed({ item, label });
  };

  const confirmRemove = async () => {
    if (!doomed) return;
    await removeItem(doomed.item.id);
    setDoomed(null);
  };

  const discard = async () => {
    if (await deleteBatch(batch.id)) navigate(back.path);
    else setDiscarding(false);
  };

  return (
    <div className="mt-6 space-y-6">
      <p className="text-muted">
        Add photos of one item to each group: the item itself, its label or serial plate, the
        receipt. Then submit, and review what the AI suggests before anything is saved.
      </p>
      <ol className="space-y-4">
        {batch.items.map((item, index) => (
          <li key={item.id}>
            <IngestItemGroup
              item={item}
              index={index}
              removable={batch.items.length > 1}
              onRemoveItem={(target) => askToRemove(target, `Item ${index + 1}`)}
              onRemovePhoto={(id) => void removePhoto(id)}
              busy={loading}
            />
          </li>
        ))}
      </ol>
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={() => void addItem(batch.id)}
          disabled={loading}
          className={SECONDARY_ACTION}
        >
          Next item
        </button>
        <button
          type="button"
          onClick={() => void submit(batch.id)}
          disabled={photos === 0 || loading}
          className={PRIMARY_ACTION}
        >
          Submit for analysis
        </button>
        <button
          type="button"
          onClick={() => setDiscarding(true)}
          disabled={loading}
          className={DANGER_ACTION}
        >
          Discard batch
        </button>
      </div>
      <ConfirmDialog
        open={doomed !== null}
        title={`Remove ${doomed?.label ?? 'item'}?`}
        body="Its photos are removed too."
        confirmLabel="Remove item"
        onConfirm={() => void confirmRemove()}
        onCancel={() => setDoomed(null)}
        busy={loading}
      />
      <ConfirmDialog
        open={discarding}
        title="Discard this batch?"
        body="Every photo in it is removed. This cannot be undone."
        confirmLabel="Discard batch"
        onConfirm={() => void discard()}
        onCancel={() => setDiscarding(false)}
        busy={loading}
      />
    </div>
  );
};
