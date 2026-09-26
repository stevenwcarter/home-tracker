import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act } from '@testing-library/react';
import { useDeleteAttachment, useSetPrimaryPhoto } from '../useAttachmentMutations';
import { DELETE_ATTACHMENT, SET_PRIMARY_PHOTO } from '../queries';
import { entityDetail } from 'test/entityFixtures';
import { REFETCHED_SUMMARY, setupWithSummary } from 'test/mutationHarness';

vi.mock('react-toastify', () => ({ toast: { error: vi.fn() } }));
import { toast } from 'react-toastify';

beforeEach(() => vi.clearAllMocks());

describe('useDeleteAttachment', () => {
  it('deletes, resolves true and refetches the summary', async () => {
    const { result, refetched } = await setupWithSummary(useDeleteAttachment, [
      {
        request: { query: DELETE_ATTACHMENT, variables: { id: 'manual' } },
        result: { data: { deleteAttachment: true } },
      },
    ]);
    let deleted: unknown;
    await act(async () => {
      deleted = await result.current.hook.remove('manual');
    });
    expect(deleted).toBe(true);
    expect(refetched).toHaveBeenCalledTimes(1);
    expect(result.current.summary).toEqual(REFETCHED_SUMMARY);
  });

  it('toasts "Could not delete" and resolves false on failure', async () => {
    const { result } = await setupWithSummary(useDeleteAttachment, [
      {
        request: { query: DELETE_ATTACHMENT, variables: { id: 'gone' } },
        result: { errors: [{ message: 'attachment not found' }] },
      },
    ]);
    let deleted: unknown;
    await act(async () => {
      deleted = await result.current.hook.remove('gone');
    });
    expect(deleted).toBe(false);
    expect(toast.error).toHaveBeenCalledWith('Could not delete: attachment not found');
  });
});

describe('useSetPrimaryPhoto', () => {
  it('returns the entity and refetches the summary', async () => {
    const drill = entityDetail({ id: 'drill', name: 'Drill' });
    const { result, refetched } = await setupWithSummary(useSetPrimaryPhoto, [
      {
        request: { query: SET_PRIMARY_PHOTO, variables: { attachmentId: 'photo2' } },
        result: { data: { setPrimaryPhoto: drill } },
      },
    ]);
    let entity: unknown;
    await act(async () => {
      entity = await result.current.hook.setPrimary('photo2');
    });
    expect(entity).toEqual(drill);
    expect(refetched).toHaveBeenCalledTimes(1);
  });

  it('toasts "Could not save" and resolves null on failure', async () => {
    const { result } = await setupWithSummary(useSetPrimaryPhoto, [
      {
        request: { query: SET_PRIMARY_PHOTO, variables: { attachmentId: 'manual' } },
        result: { errors: [{ message: 'attachment is not a photo' }] },
      },
    ]);
    let entity: unknown;
    await act(async () => {
      entity = await result.current.hook.setPrimary('manual');
    });
    expect(entity).toBeNull();
    expect(toast.error).toHaveBeenCalledWith('Could not save: attachment is not a photo');
  });
});
