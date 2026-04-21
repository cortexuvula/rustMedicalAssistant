import { invoke } from '@tauri-apps/api/core';

export interface ContextTemplate {
  name: string;
  body: string;
}

export function listContextTemplates(): Promise<ContextTemplate[]> {
  return invoke('list_context_templates');
}

export function upsertContextTemplate(
  name: string,
  body: string,
): Promise<ContextTemplate> {
  return invoke('upsert_context_template', { name, body });
}

export function renameContextTemplate(
  oldName: string,
  newName: string,
): Promise<ContextTemplate> {
  return invoke('rename_context_template', { oldName, newName });
}

export function deleteContextTemplate(name: string): Promise<void> {
  return invoke('delete_context_template', { name });
}

export function importContextTemplatesJson(filePath: string): Promise<number> {
  return invoke('import_context_templates_json', { filePath });
}

export function exportContextTemplatesJson(filePath: string): Promise<number> {
  return invoke('export_context_templates_json', { filePath });
}
