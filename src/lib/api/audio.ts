import { invoke } from '@tauri-apps/api/core';
import type { AudioDevice } from '../types';

export async function listAudioDevices(): Promise<AudioDevice[]> {
  return invoke('list_audio_devices');
}

export async function startRecording(): Promise<string> {
  return invoke('start_recording');
}

export async function stopRecording(): Promise<string> {
  return invoke('stop_recording');
}

export async function cancelRecording(): Promise<void> {
  return invoke('cancel_recording');
}

export async function pauseRecording(): Promise<void> {
  return invoke('pause_recording');
}

export async function resumeRecording(): Promise<void> {
  return invoke('resume_recording');
}

export interface RecordingAudioLevels {
  peak: number;
  rms: number;
  is_silent: boolean;
}

export async function checkRecordingAudioLevels(
  recordingId: string,
): Promise<RecordingAudioLevels> {
  return invoke('check_recording_audio_levels', { recordingId });
}
