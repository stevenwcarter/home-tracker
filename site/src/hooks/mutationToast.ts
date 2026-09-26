import { toast } from 'react-toastify';

/** What the user tried to do, as `Could not <verb>` phrases it. */
export type FailureVerb =
  | 'save'
  | 'delete'
  | 'test the connection'
  | 'start the batch'
  | 'add an item'
  | 'remove the item'
  | 'remove the photo'
  | 'submit the batch'
  | 'retry'
  | 'skip';

/**
 * Runs a write; on failure toasts `Could not <verb>: <server message>` and
 * resolves to `fallback` instead of rejecting, so a caller can branch on the
 * result (`if (created) navigate(...)`) without a try/catch of its own.
 */
export async function toastOnFailure<T, F>(
  action: () => Promise<T>,
  verb: FailureVerb,
  fallback: F,
): Promise<T | F> {
  try {
    return await action();
  } catch (error) {
    toast.error(`Could not ${verb}: ${error instanceof Error ? error.message : String(error)}`);
    return fallback;
  }
}
