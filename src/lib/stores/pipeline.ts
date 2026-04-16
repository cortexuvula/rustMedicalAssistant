import { writable } from 'svelte/store';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { processRecording } from '../api/pipeline';
import { recordings } from './recordings';
import { log } from '../api/logging';

export type PipelineStage = 'idle' | 'transcribing' | 'generating_soap' | 'completed' | 'failed';

export interface PipelineEntry {
  recordingId: string;
  stage: PipelineStage;
  error: string | null;
}

interface PipelineState {
  /** The most recent pipeline (shown on Record tab). */
  current: PipelineEntry | null;
  /** All active pipelines keyed by recording ID. */
  active: Record<string, PipelineEntry>;
}

function createPipelineStore() {
  const { subscribe, update } = writable<PipelineState>({
    current: null,
    active: {},
  });

  let progressUnlisten: UnlistenFn | null = null;

  return {
    subscribe,

    /** Start listening for backend pipeline events. Call once on app mount. */
    async init() {
      progressUnlisten = await listen<{ recording_id: string; stage: string; error?: string }>(
        'pipeline-progress',
        (event) => {
          const { recording_id, stage, error } = event.payload;
          const entry: PipelineEntry = {
            recordingId: recording_id,
            stage: stage as PipelineStage,
            error: error ?? null,
          };
          update((s) => ({
            ...s,
            current: s.current?.recordingId === recording_id ? entry : s.current,
            active: { ...s.active, [recording_id]: entry },
          }));

          // Clean up completed/failed entries from active map after a delay
          if (stage === 'completed' || stage === 'failed') {
            if (stage === 'failed') {
              log.error('Pipeline failed', { recording_id, error: error ?? 'unknown' });
            } else {
              log.info('Pipeline completed', { recording_id });
            }
            recordings.load(); // Refresh recordings list
            setTimeout(() => {
              update((s) => {
                const { [recording_id]: _, ...rest } = s.active;
                return { ...s, active: rest };
              });
            }, 30000);
          }
        },
      );
    },

    /** Launch the pipeline for a recording. Non-blocking — returns immediately. */
    launch(recordingId: string, context?: string, template?: string) {
      const entry: PipelineEntry = {
        recordingId,
        stage: 'transcribing',
        error: null,
      };
      update((s) => ({
        ...s,
        current: entry,
        active: { ...s.active, [recordingId]: entry },
      }));

      log.info('Pipeline launched', { recordingId, hasContext: !!context, template: template ?? 'default' });

      // Fire and forget — progress comes via events
      processRecording(recordingId, context, template).catch((err) => {
        log.error('Pipeline command failed', { recordingId, error: String(err) });
        const errorEntry: PipelineEntry = {
          recordingId,
          stage: 'failed',
          error: String(err),
        };
        update((s) => ({
          ...s,
          current: s.current?.recordingId === recordingId ? errorEntry : s.current,
          active: { ...s.active, [recordingId]: errorEntry },
        }));
      });
    },

    /** Clear the current pipeline display (e.g., when starting a new recording). */
    clearCurrent() {
      update((s) => ({ ...s, current: null }));
    },

    /** Retry a failed pipeline. */
    retry(recordingId: string, context?: string, template?: string) {
      this.launch(recordingId, context, template);
    },

    destroy() {
      progressUnlisten?.();
    },
  };
}

export const pipeline = createPipelineStore();
