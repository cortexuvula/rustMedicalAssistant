import { invoke } from '@tauri-apps/api/core';

export async function generateSoap(
  recordingId: string,
  template?: string
): Promise<string> {
  return invoke('generate_soap', { recordingId, template });
}

export async function generateReferral(
  recordingId: string,
  recipientType?: string,
  urgency?: string
): Promise<string> {
  return invoke('generate_referral', { recordingId, recipientType, urgency });
}

export async function generateLetter(
  recordingId: string,
  letterType?: string
): Promise<string> {
  return invoke('generate_letter', { recordingId, letterType });
}

export async function generateSynopsis(
  recordingId: string
): Promise<string> {
  return invoke('generate_synopsis', { recordingId });
}
