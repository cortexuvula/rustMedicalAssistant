import { writable } from 'svelte/store';
import {
  listContextTemplates,
  type ContextTemplate,
} from '../api/contextTemplates';

function createContextTemplatesStore() {
  const { subscribe, set } = writable<ContextTemplate[]>([]);

  return {
    subscribe,

    async load(): Promise<void> {
      try {
        const items = await listContextTemplates();
        set(items);
      } catch (err) {
        console.error('Failed to load context templates:', err);
      }
    },
  };
}

export const contextTemplates = createContextTemplatesStore();
