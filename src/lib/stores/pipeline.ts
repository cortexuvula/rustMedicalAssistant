import { writable } from 'svelte/store';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { processRecording, cancelPipeline } from '../api/pipeline';
import { recordings } from './recordings';
import { log } from '../api/logging';
import { formatError } from '../types/errors';

export type PipelineStage = 'idle' | 'transcribing' | 'generating_soap' | 'completed' | 'failed';

export interface PipelineEntry {
  recordingId: string;
  stage: PipelineStage;
  error: string | null;
  /** Wall-clock ms at pipeline launch — for the elapsed-time counter. */
  startedAt: number;
  /** Wall-clock ms when the stage reached `completed` or `failed`. Null while in-flight. */
  finishedAt: number | null;
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

  // Track pending 30s cleanup timers per recording-id so we can cancel them
  // if the pipeline is re-launched or removed before the timer fires. Without
  // this, a stale timer from a prior run could clobber a freshly launched
  // entry for the same recording id.
  const pendingCleanups = new Map<string, ReturnType<typeof setTimeout>>();

  function scheduleCleanup(recordingId: string, delayMs: number) {
    // Cancel any existing cleanup for this recording before scheduling a new one.
    const existing = pendingCleanups.get(recordingId);
    if (existing) clearTimeout(existing);

    // Capture the id by value via the function parameter (which is a fresh
    // binding on each call), so the closure can't see a reassigned outer var.
    const id = recordingId;
    const handle = setTimeout(() => {
      pendingCleanups.delete(id);
      update((s) => {
        // Only remove if the entry is still in a terminal state — a
        // re-launched pipeline for the same recording ID should not be
        // cleaned up by a stale timer from the previous run.
        const existingEntry = s.active[id];
        if (!existingEntry || existingEntry.stage === 'completed' || existingEntry.stage === 'failed') {
          const { [id]: _, ...rest } = s.active;
          return { ...s, active: rest };
        }
        return s;
      });
    }, delayMs);

    pendingCleanups.set(id, handle);
  }

  return {
    subscribe,

    /** Start listening for backend pipeline events. Call once on app mount. */
    async init() {
      progressUnlisten = await listen<{ recording_id: string; stage: string; error?: string }>(
        'pipeline-progress',
        (event) => {
          const { recording_id, stage, error } = event.payload;
          const isTerminal = stage === 'completed' || stage === 'failed';
          update((s) => {
            const prior = s.active[recording_id];
            const entry: PipelineEntry = {
              recordingId: recording_id,
              stage: stage as PipelineStage,
              error: error ?? null,
              // Preserve the launch timestamp across stage transitions. If we
              // missed the launch (e.g. HMR reloaded the store mid-pipeline),
              // fall back to now — ETA will be slightly off but usable.
              startedAt: prior?.startedAt ?? Date.now(),
              // Freeze the clock when we hit a terminal state.
              finishedAt: isTerminal
                ? (prior?.finishedAt ?? Date.now())
                : null,
            };
            return {
              ...s,
              current: s.current?.recordingId === recording_id ? entry : s.current,
              active: { ...s.active, [recording_id]: entry },
            };
          });

          // Clean up completed/failed entries from active map after a delay
          if (stage === 'completed' || stage === 'failed') {
            if (stage === 'failed') {
              log.error('Pipeline failed', { recording_id, error: error ?? 'unknown' });
            } else {
              log.info('Pipeline completed', { recording_id });
            }
            recordings.load(); // Refresh recordings list
            scheduleCleanup(recording_id, 30000);
          }
        },
      );
    },

    /** Launch the pipeline for a recording. Non-blocking — returns immediately. */
    launch(recordingId: string, context?: string, template?: string) {
      // If a prior pipeline for this recording id still has a pending cleanup
      // timer, cancel it — otherwise the stale timer could delete this fresh
      // entry once it fires.
      const pending = pendingCleanups.get(recordingId);
      if (pending) {
        clearTimeout(pending);
        pendingCleanups.delete(recordingId);
      }

      const startedAt = Date.now();
      const entry: PipelineEntry = {
        recordingId,
        stage: 'transcribing',
        error: null,
        startedAt,
        finishedAt: null,
      };
      update((s) => ({
        ...s,
        current: entry,
        active: { ...s.active, [recordingId]: entry },
      }));

      log.info('Pipeline launched', { recordingId, hasContext: !!context, template: template ?? 'default' });

      // Fire and forget — progress comes via events
      processRecording(recordingId, context, template).catch((err) => {
        const message = formatError(err);
        log.error('Pipeline command failed', { recordingId, error: message });
        update((s) => {
          const prior = s.active[recordingId];
          const errorEntry: PipelineEntry = {
            recordingId,
            stage: 'failed',
            error: message,
            startedAt: prior?.startedAt ?? startedAt,
            finishedAt: Date.now(),
          };
          return {
            ...s,
            current: s.current?.recordingId === recordingId ? errorEntry : s.current,
            active: { ...s.active, [recordingId]: errorEntry },
          };
        });
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

    /** Signal a running pipeline to cancel at its next stage boundary. */
    async cancel(recordingId: string) {
      try {
        const ok = await cancelPipeline(recordingId);
        log.info('Pipeline cancel requested', { recordingId, found: ok });
      } catch (err) {
        log.error('Pipeline cancel failed', { recordingId, error: formatError(err) });
      }
    },

    destroy() {
      progressUnlisten?.();
      // Cancel any outstanding cleanup timers so they don't fire against a
      // torn-down store.
      for (const handle of pendingCleanups.values()) {
        clearTimeout(handle);
      }
      pendingCleanups.clear();
    },
  };
}

export const pipeline = createPipelineStore();
