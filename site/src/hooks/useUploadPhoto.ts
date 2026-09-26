import { useApolloClient } from '@apollo/client/react';
import { useCallback, useState } from 'react';
import { toast } from 'react-toastify';
import { NETWORK_ERROR_MESSAGE, preflightError, uploadErrorMessage } from 'utils/uploadErrors';
import { refetchAfterWrite } from './useRefetchingMutation';

/** The queries a new photo can change: the open page, list thumbnails and the summary. */
const REFETCH_AFTER_UPLOAD = ['GetEntity', 'GetSummary', 'GetRootItems', 'GetLocations'] as const;

export type UploadStatus = 'queued' | 'uploading' | 'done' | 'error';

/** Where one file of the current batch stands; `error` is set only with status `error`. */
export interface UploadProgress {
  file: File;
  status: UploadStatus;
  error?: string;
}

/** The outcome of one file; `error` is the user-facing message when `ok` is false. */
export interface UploadResult {
  file: File;
  ok: boolean;
  error?: string;
}

export interface UploadOptions {
  /** Make the first file of the batch the entity's primary photo. */
  primary?: boolean;
}

/** Reads the `{ "error": ... }` body of a failed upload, or null when there is none. */
const serverError = async (response: Response): Promise<string | null> => {
  try {
    const body: unknown = await response.json();
    if (body && typeof body === 'object' && 'error' in body && typeof body.error === 'string') {
      return body.error;
    }
  } catch {
    // Not JSON (a proxy's HTML error page, say): fall back to the status.
  }
  return null;
};

/**
 * Posts one file; resolves to its result and never rejects. A file over the
 * size cap is refused here without a request (see `preflightError`).
 */
const postPhoto = async (entityId: string, file: File, primary: boolean): Promise<UploadResult> => {
  const refused = preflightError(file);
  if (refused) return { file, ok: false, error: refused };
  const form = new FormData();
  form.append('file', file);
  if (primary) form.append('primary', 'true');
  let response: Response;
  try {
    response = await fetch(`/api/upload/${encodeURIComponent(entityId)}`, {
      method: 'POST',
      body: form,
    });
  } catch {
    return { file, ok: false, error: NETWORK_ERROR_MESSAGE };
  }
  if (response.ok) return { file, ok: true };
  return {
    file,
    ok: false,
    error: uploadErrorMessage(response.status, await serverError(response)),
  };
};

/**
 * Uploads photos to `entity` through `POST /api/upload/{id}` (plain `fetch`,
 * not Apollo: the endpoint is multipart, not GraphQL).
 *
 * `upload(files, { primary })` sends one request per file, strictly one after
 * another, so the backend's "first photo becomes primary" rule sees them in
 * order; `primary` asks for the first file to become primary. It resolves to
 * one result per file and never rejects; each failure is also toasted. Once
 * the batch ends, if anything was stored, it applies the shared refetch policy
 * (`refetchAfterWrite`): the entity and its parent are evicted, and
 * `GetEntity`, `GetSummary`, `GetRootItems` and `GetLocations` are refetched
 * when mounted or evicted when not.
 *
 * `progress` holds the latest batch's per-file status. `fetch` cannot observe
 * upload bytes, so it is a status, not a percentage.
 */
export const useUploadPhoto = (entity: { id: string; parentId: string | null }) => {
  const client = useApolloClient();
  const [progress, setProgress] = useState<UploadProgress[]>([]);
  const [uploading, setUploading] = useState(false);
  const { id, parentId } = entity;

  const upload = useCallback(
    async (files: File[], options: UploadOptions = {}): Promise<UploadResult[]> => {
      const mark = (index: number, next: Omit<UploadProgress, 'file'>) =>
        setProgress((current) =>
          current.map((entry, at) => (at === index ? { file: entry.file, ...next } : entry)),
        );
      setProgress(files.map((file) => ({ file, status: 'queued' })));
      setUploading(true);
      const results: UploadResult[] = [];
      try {
        for (const [index, file] of files.entries()) {
          mark(index, { status: 'uploading' });
          const result = await postPhoto(id, file, index === 0 && options.primary === true);
          results.push(result);
          if (result.ok) {
            mark(index, { status: 'done' });
          } else {
            mark(index, { status: 'error', error: result.error });
            toast.error(`Could not upload ${file.name}: ${result.error}`);
          }
        }
        if (results.some((result) => result.ok)) {
          // A failed refetch is the query's own error to show (its hook toasts
          // it); the photos are stored, so `upload` still reports success.
          await refetchAfterWrite(client, REFETCH_AFTER_UPLOAD, [id, parentId]).catch(() => {});
        }
      } finally {
        setUploading(false);
      }
      return results;
    },
    [client, id, parentId],
  );

  return { upload, uploading, progress };
};
