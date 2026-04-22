import { invoke } from '@tauri-apps/api/core';

export async function processRecording(
  recordingId: string,
  context?: string,
  template?: string,
): Promise<string> {
  return invoke('process_recording', {
    recordingId,
    context: context ?? null,
    template: template ?? null,
  });
}

export async function cancelPipeline(recordingId: string): Promise<boolean> {
  return invoke('cancel_pipeline', { recordingId });
}
