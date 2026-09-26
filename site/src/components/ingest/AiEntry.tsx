import { Link, useNavigate } from 'react-router-dom';
import { SECONDARY_ACTION } from 'components/buttonStyles';
import { useAiSettings } from 'hooks/useAiSettings';
import { useIngestMutations } from 'hooks/useIngestMutations';
import { useOpenIngestBatches } from 'hooks/useOpenIngestBatches';
import { timeAgo } from 'utils/date';
import { plural } from 'utils/plural';

/**
 * The way into AI ingest from an entity's page: "Add item(s) with AI" (start
 * a batch with `parentId` as the default parent, and open it) when an API key
 * is configured, else a muted "Set up AI" link to the settings tab; below it,
 * a "Resume batch" link for each unfinished batch started here.
 */
export const AiEntry = ({ parentId }: { parentId: string }) => {
  const { settings } = useAiSettings();
  const { batches } = useOpenIngestBatches(parentId);
  const { createBatch, loading } = useIngestMutations();
  const navigate = useNavigate();

  const start = async () => {
    const batch = await createBatch(parentId);
    // On failure the hook has toasted; stay on the page.
    if (batch) navigate(`/ingest/${batch.id}`);
  };

  return (
    <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-2">
      {settings?.hasApiKey ? (
        <button
          type="button"
          onClick={() => void start()}
          disabled={loading}
          className={SECONDARY_ACTION}
        >
          Add item(s) with AI
        </button>
      ) : (
        settings && (
          <Link to="/settings/ai" className="text-sm text-muted hover:text-text">
            Set up AI
          </Link>
        )
      )}
      {batches.length > 0 && (
        <ul aria-label="Unfinished AI batches" className="flex flex-wrap gap-x-4 gap-y-1">
          {batches.map((batch) => (
            <li key={batch.id}>
              <Link to={`/ingest/${batch.id}`} className="text-sm text-accent hover:underline">
                Resume batch{' '}
                <span className="text-muted">
                  ({plural(batch.items.length, 'item', 'items')}, started {timeAgo(batch.createdAt)}
                  )
                </span>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
};
