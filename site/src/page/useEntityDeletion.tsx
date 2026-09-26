import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { ConfirmDialog } from 'components/ConfirmDialog';
import { useDeleteEntity } from 'hooks/useEntityMutations';
import { EntityDetail } from 'types/entity';

/**
 * The Delete flow for an entity page: `askToDelete` opens a `ConfirmDialog`
 * (rendered by the page as `dialog`); confirming deletes the entity and, in
 * the same tick the delete resolves, replaces the route with the parent's page
 * (or home). The delete evicts the entity, so its still-mounted page refetches
 * and would briefly render "Not found"; `deleting` stays true from confirm
 * until the page is left, so the page can show a placeholder instead.
 *
 * Lives with the page (not in a child component) because the page swaps its
 * content for that placeholder, which would unmount a child mid-delete.
 */
export const useEntityDeletion = (entity: EntityDetail | null) => {
  const navigate = useNavigate();
  const { remove, loading } = useDeleteEntity();
  const [confirming, setConfirming] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const confirm = async () => {
    if (!entity) return;
    const { id, parentId } = entity;
    setDeleting(true);
    if (await remove({ id, parentId })) {
      // The parent's route can reuse this page instance (`/locations/:id` to
      // `/locations/:parent`), so the dialog state must not carry over to it.
      setConfirming(false);
      setDeleting(false);
      // A parent that is an item redirects from `/locations/:id` to its item page.
      navigate(parentId ? `/locations/${parentId}` : '/', { replace: true });
      return;
    }
    // The hook has toasted the reason (e.g. the location still holds things).
    setDeleting(false);
    setConfirming(false);
  };

  const kind = entity?.isLocation ? 'location' : 'item';
  const dialog = entity && (
    <ConfirmDialog
      open={confirming}
      title={`Delete ${entity.name}?`}
      body={
        entity.isLocation
          ? 'This cannot be undone. A location must be empty before it can be deleted.'
          : 'This cannot be undone.'
      }
      confirmLabel={`Delete ${kind}`}
      onConfirm={confirm}
      onCancel={() => setConfirming(false)}
      busy={loading || deleting}
    />
  );

  return { askToDelete: () => setConfirming(true), deleting, dialog };
};
