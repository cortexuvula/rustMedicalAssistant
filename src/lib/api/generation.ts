import { invoke } from '@tauri-apps/api/core';

export async function generateSoap(
  recordingId: string,
  template?: string
): Promise<string> {
  return invoke('generate_soap', { recording_id: recordingId, template });
}

export async function generateReferral(
  recordingId: string,
  recipientType?: string,
  urgency?: string
): Promise<string> {
  return invoke('generate_referral', { recording_id: recordingId, recipient_type: recipientType, urgency });
}

export async function generateLetter(
  recordingId: string,
  letterType?: string
): Promise<string> {
  return invoke('generate_letter', { recording_id: recordingId, letter_type: letterType });
}

export async function generateSynopsis(
  recordingId: string
): Promise<string> {
  return invoke('generate_synopsis', { recording_id: recordingId });
}
