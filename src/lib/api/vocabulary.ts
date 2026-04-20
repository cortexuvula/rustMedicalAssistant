import { invoke } from '@tauri-apps/api/core';

export interface VocabularyEntry {
  id: string;
  find_text: string;
  replacement: string;
  category: string;
  case_sensitive: boolean;
  priority: number;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface AppliedCorrection {
  find_text: string;
  replacement: string;
  category: string;
  count: number;
}

export interface CorrectionResult {
  original_text: string;
  corrected_text: string;
  corrections_applied: AppliedCorrection[];
  total_replacements: number;
}

export async function listVocabularyEntries(category?: string): Promise<VocabularyEntry[]> {
  return invoke('list_vocabulary_entries', { category: category ?? null });
}

export async function addVocabularyEntry(
  findText: string,
  replacement: string,
  category?: string,
  caseSensitive?: boolean,
  priority?: number,
  enabled?: boolean,
): Promise<VocabularyEntry> {
  return invoke('add_vocabulary_entry', {
    findText,
    replacement,
    category: category ?? null,
    caseSensitive: caseSensitive ?? null,
    priority: priority ?? null,
    enabled: enabled ?? null,
  });
}

export async function updateVocabularyEntry(
  id: string,
  findText: string,
  replacement: string,
  category?: string,
  caseSensitive?: boolean,
  priority?: number,
  enabled?: boolean,
): Promise<VocabularyEntry> {
  return invoke('update_vocabulary_entry', {
    id,
    findText,
    replacement,
    category: category ?? null,
    caseSensitive: caseSensitive ?? null,
    priority: priority ?? null,
    enabled: enabled ?? null,
  });
}

export async function deleteVocabularyEntry(id: string): Promise<void> {
  return invoke('delete_vocabulary_entry', { id });
}

export async function deleteAllVocabularyEntries(): Promise<number> {
  return invoke('delete_all_vocabulary_entries');
}

export async function getVocabularyCount(): Promise<[number, number]> {
  return invoke('get_vocabulary_count');
}

export async function importVocabularyJson(filePath: string): Promise<number> {
  return invoke('import_vocabulary_json', { filePath });
}

export async function exportVocabularyJson(filePath: string): Promise<number> {
  return invoke('export_vocabulary_json', { filePath });
}

export async function testVocabularyCorrection(text: string): Promise<CorrectionResult> {
  return invoke('test_vocabulary_correction', { text });
}
