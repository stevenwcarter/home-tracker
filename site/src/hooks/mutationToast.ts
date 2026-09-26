import { toast } from 'react-toastify';

/**
 * Runs a write; on failure toasts `Could not <verb>: <server message>` and
 * resolves to `fallback` instead of rejecting, so a caller can branch on the
 * result (`if (created) navigate(...)`) without a try/catch of its own.
 */
export async function toastOnFailure<T, F>(
  action: () => Promise<T>,
  verb: 'save' | 'delete' | 'test the connection',
  fallback: F,
): Promise<T | F> {
  try {
    return await action();
  } catch (error) {
    toast.error(`Could not ${verb}: ${error instanceof Error ? error.message : String(error)}`);
    return fallback;
  }
}
