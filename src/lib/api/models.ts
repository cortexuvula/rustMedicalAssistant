import { invoke } from '@tauri-apps/api/core';

export interface ModelInfo {
  id: string;
  filename: string;
  size_bytes: number;
  download_url: string;
  description: string;
  downloaded: boolean;
}

export async function listWhisperModels(): Promise<ModelInfo[]> {
  return invoke('list_whisper_models');
}

export async function listPyannoteModels(): Promise<ModelInfo[]> {
  return invoke('list_pyannote_models');
}

export async function downloadModel(modelId: string): Promise<void> {
  return invoke('download_model', { modelId });
}

export async function deleteModel(modelId: string): Promise<void> {
  return invoke('delete_model', { modelId });
}
