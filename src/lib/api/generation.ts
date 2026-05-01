import { invoke } from '@tauri-apps/api/core';
import type { PatientContext } from '../types';

export async function generateSoap(
  recordingId: string,
  template?: string,
  context?: string,
  patientContext?: PatientContext,
): Promise<string> {
  // Tauri omits undefined fields from the payload, so explicitly pass null
  // for optional parameters to ensure they map to Rust Option::None
  return invoke('generate_soap', {
    recordingId,
    template: template ?? null,
    context: context ?? null,
    patientContext: patientContext ?? null,
  });
}

export async function generateReferral(
  recordingId: string,
  recipientType?: string,
  urgency?: string
): Promise<string> {
  return invoke('generate_referral', {
    recordingId,
    recipientType: recipientType ?? null,
    urgency: urgency ?? null,
  });
}

export async function generateLetter(
  recordingId: string,
  letterType?: string
): Promise<string> {
  return invoke('generate_letter', {
    recordingId,
    letterType: letterType ?? null,
  });
}

export async function generateSynopsis(
  recordingId: string
): Promise<string> {
  return invoke('generate_synopsis', { recordingId });
}
