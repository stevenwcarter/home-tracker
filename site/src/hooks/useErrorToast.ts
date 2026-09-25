import { useEffect } from 'react';
import { toast } from 'react-toastify';

/** Toasts `message` once whenever `error` becomes truthy. */
export function useErrorToast(error: unknown, message: string): void {
  useEffect(() => {
    if (error) toast.error(message);
  }, [error, message]);
}
