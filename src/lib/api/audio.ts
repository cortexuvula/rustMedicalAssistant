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

export async function pauseRecording(): Promise<void> {
  return invoke('pause_recording');
}

export async function resumeRecording(): Promise<void> {
  return invoke('resume_recording');
}
