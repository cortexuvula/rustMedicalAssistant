import { invoke } from '@tauri-apps/api/core';
import type { Recording, RecordingSummary } from '../types';

export async function listRecordings(limit = 50, offset = 0): Promise<RecordingSummary[]> {
  return invoke('list_recordings', { limit, offset });
}

export async function getRecording(id: string): Promise<Recording> {
  return invoke('get_recording', { id });
}

export async function searchRecordings(query: string, limit = 20): Promise<Recording[]> {
  return invoke('search_recordings', { query, limit });
}

export async function deleteRecording(id: string): Promise<void> {
  return invoke('delete_recording', { id });
}
