import { useApolloClient } from '@apollo/client/react';
import { useCallback, useState } from 'react';
import { toast } from 'react-toastify';
import { NETWORK_ERROR_MESSAGE, preflightError, uploadErrorMessage } from 'utils/uploadErrors';
import { refetchAfterWrite } from './useRefetchingMutation';

/** The queries a new photo can change: the open page, list thumbnails and the summary. */
const REFETCH_AFTER_UPLOAD = ['GetEntity', 'GetSummary', 'GetRootItems', 'GetLocations'] as const;

/** A staged photo changes only its batch: nothing outside the batch sees it until accepted. */
const REFETCH_AFTER_STAGING = ['GetIngestBatch'] as const;

/**
 * Where uploaded photos go: onto an entity as attachments (`/api/upload/{id}`),
 * or staged on an AI ingest item (`/api/ingest/items/{id}/photos`).
 */
export type UploadTarget =
  | { kind: 'entity'; entity: { id: string; parentId: string | null } }
  | { kind: 'ingestItem'; itemId: string };

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
  /** Make the first file of the batch the entity's primary photo; ignored for an ingest item. */
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

/** The endpoint an upload to an entity (`isEntity`) or an ingest item `id` posts to. */
const uploadUrl = (isEntity: boolean, id: string): string =>
  isEntity
    ? `/api/upload/${encodeURIComponent(id)}`
    : `/api/ingest/items/${encodeURIComponent(id)}/photos`;

/**
 * Posts one file; resolves to its result and never rejects. A file over the
 * size cap is refused here without a request (see `preflightError`).
 */
const postPhoto = async (url: string, file: File, primary: boolean): Promise<UploadResult> => {
  const refused = preflightError(file);
  if (refused) return { file, ok: false, error: refused };
  const form = new FormData();
  form.append('file', file);
  if (primary) form.append('primary', 'true');
  let response: Response;
  try {
    response = await fetch(url, {
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
 * Uploads photos to `target` (plain `fetch`, not Apollo: the endpoints are
 * multipart, not GraphQL): an entity's `POST /api/upload/{id}`, or an ingest
 * item's `POST /api/ingest/items/{id}/photos`.
 *
 * `upload(files, { primary })` sends one request per file, strictly one after
 * another, so the backend's "first photo becomes primary" rule (and an ingest
 * item's photo order) sees them in order; `primary` asks for the first file
 * to become an entity's primary photo. It resolves to one result per file and
 * never rejects; each failure is also toasted. Once the batch ends, if
 * anything was stored, it applies the shared refetch policy
 * (`refetchAfterWrite`): for an entity, the entity and its parent are evicted,
 * and `GetEntity`, `GetSummary`, `GetRootItems` and `GetLocations` are
 * refetched when mounted or evicted when not; for an ingest item, only
 * `GetIngestBatch`.
 *
 * `progress` holds the latest batch's per-file status. `fetch` cannot observe
 * upload bytes, so it is a status, not a percentage.
 */
export const useUploadPhoto = (target: UploadTarget) => {
  const client = useApolloClient();
  const [progress, setProgress] = useState<UploadProgress[]>([]);
  const [uploading, setUploading] = useState(false);
  // Primitives, so an inline `target` object doesn't give `upload` a new identity per render.
  const isEntity = target.kind === 'entity';
  const id = isEntity ? target.entity.id : target.itemId;
  const parentId = isEntity ? target.entity.parentId : null;

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
          const primary = isEntity && index === 0 && options.primary === true;
          const result = await postPhoto(uploadUrl(isEntity, id), file, primary);
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
          const refetched = isEntity
            ? refetchAfterWrite(client, REFETCH_AFTER_UPLOAD, [id, parentId])
            : refetchAfterWrite(client, REFETCH_AFTER_STAGING, []);
          await refetched.catch(() => {});
        }
      } finally {
        setUploading(false);
      }
      return results;
    },
    [client, isEntity, id, parentId],
  );

  return { upload, uploading, progress };
};
