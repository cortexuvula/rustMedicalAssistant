import { writable } from 'svelte/store';
import type { Recording, RecordingSummary } from '../types';
import {
  listRecordings,
  getRecording,
  searchRecordings,
  deleteRecording,
  deleteAllRecordings,
} from '../api/recordings';

function createRecordingsStore() {
  const { subscribe, set, update } = writable<RecordingSummary[]>([]);

  return {
    subscribe,

    async load(limit = 50, offset = 0): Promise<void> {
      loading.set(true);
      try {
        const items = await listRecordings(limit, offset);
        set(items);
      } catch (err) {
        console.error('Failed to load recordings:', err);
      } finally {
        loading.set(false);
      }
    },

    async search(query: string): Promise<void> {
      searchQuery.set(query);
      loading.set(true);
      try {
        if (query.trim() === '') {
          const items = await listRecordings();
          set(items);
        } else {
          const results = await searchRecordings(query);
          // Map full Recording to RecordingSummary shape
          const summaries: RecordingSummary[] = results.map((r) => ({
            id: r.id,
            filename: r.filename,
            patient_name: r.patient_name,
            status: r.status,
            duration_seconds: r.duration_seconds,
            created_at: r.created_at,
            tags: r.tags,
            has_transcript: r.transcript !== null,
            has_soap_note: r.soap_note !== null,
            has_referral: r.referral !== null,
            has_letter: r.letter !== null,
          }));
          set(summaries);
        }
      } catch (err) {
        console.error('Failed to search recordings:', err);
      } finally {
        loading.set(false);
      }
    },

    async remove(id: string): Promise<void> {
      try {
        await deleteRecording(id);
        update((items) => items.filter((r) => r.id !== id));
        // Clear selected if it was the deleted one
        selectedRecording.update((current) =>
          current?.id === id ? null : current
        );
      } catch (err) {
        console.error('Failed to delete recording:', err);
        throw err;
      }
    },

    async removeAll(): Promise<number> {
      try {
        const count = await deleteAllRecordings();
        set([]);
        selectedRecording.set(null);
        return count;
      } catch (err) {
        console.error('Failed to delete all recordings:', err);
        throw err;
      }
    },
  };
}

export const loading = writable<boolean>(false);
export const searchQuery = writable<string>('');
export const recordings = createRecordingsStore();

export const selectedRecording = writable<Recording | null>(null);

export async function selectRecording(id: string): Promise<void> {
  try {
    const recording = await getRecording(id);
    selectedRecording.set(recording);
  } catch (err) {
    console.error('Failed to select recording:', err);
    throw err;
  }
}
