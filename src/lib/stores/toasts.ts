import { writable } from 'svelte/store';

export interface Toast {
  id: string;
  message: string;
  type: 'success' | 'error';
  /** Recording ID for "View" button navigation. */
  recordingId?: string;
  /** Display name shown in the toast. */
  displayName?: string;
  /** Whether to auto-dismiss (errors persist until manually dismissed). */
  autoDismiss: boolean;
}

function createToastStore() {
  const { subscribe, update } = writable<Toast[]>([]);
  let counter = 0;

  return {
    subscribe,

    add(toast: Omit<Toast, 'id'>) {
      const id = `toast-${++counter}`;
      const entry = { ...toast, id };
      update((toasts) => [...toasts, entry]);

      if (toast.autoDismiss) {
        setTimeout(() => {
          this.dismiss(id);
        }, 8000);
      }

      return id;
    },

    dismiss(id: string) {
      update((toasts) => toasts.filter((t) => t.id !== id));
    },
  };
}

export const toasts = createToastStore();
