import { useCallback } from 'react';
import { EntityDetail } from 'types/entity';
import { DELETE_ATTACHMENT, SET_PRIMARY_PHOTO } from './queries';
import { toastOnFailure } from './mutationToast';
import { REFETCH_AFTER_WRITE, useRefetchingMutation } from './useRefetchingMutation';

// The open entity page lists the attachments; list thumbnails show the primary photo.
const REFETCH = [...REFETCH_AFTER_WRITE, 'GetEntity'];

/** `remove(id)` resolves true once deleted, or false after toasting a failure. */
export const useDeleteAttachment = () => {
  const [mutate, { loading }] = useRefetchingMutation<
    { deleteAttachment: boolean },
    { id: string }
  >(DELETE_ATTACHMENT, { refetch: REFETCH });
  const remove = useCallback(
    (id: string) =>
      toastOnFailure(async () => (await mutate({ id })).deleteAttachment, 'delete', false),
    [mutate],
  );
  return { remove, loading };
};

/** `setPrimary(attachmentId)` resolves to the entity, or null after toasting a failure. */
export const useSetPrimaryPhoto = () => {
  const [mutate, { loading }] = useRefetchingMutation<
    { setPrimaryPhoto: EntityDetail },
    { attachmentId: string }
  >(SET_PRIMARY_PHOTO, { refetch: REFETCH });
  const setPrimary = useCallback(
    (attachmentId: string) =>
      toastOnFailure(async () => (await mutate({ attachmentId })).setPrimaryPhoto, 'save', null),
    [mutate],
  );
  return { setPrimary, loading };
};
