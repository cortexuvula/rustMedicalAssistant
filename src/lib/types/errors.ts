/** Structured error payload emitted by Tauri commands. */
export type TauriError = {
  kind:
    | "Database"
    | "Security"
    | "Audio"
    | "AiProvider"
    | "SttProvider"
    | "TtsProvider"
    | "Agent"
    | "Rag"
    | "Processing"
    | "Export"
    | "Translation"
    | "Config"
    | "Io"
    | "Serialization"
    | "Cancelled"
    | "Other";
  message: string;
};

/**
 * Best-effort extraction of a human-readable message from anything `invoke`
 * might throw. Handles the new structured shape, plain strings (legacy), and
 * unknowns.
 */
export function formatError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  if (err && typeof err === "object") {
    const e = err as Partial<TauriError> & { message?: unknown };
    if (typeof e.message === "string") return e.message;
    try {
      return JSON.stringify(err);
    } catch {
      // fall through to String() below (e.g. circular refs)
    }
  }
  return String(err);
}

/** Type guard: was this a structured AppError, not a raw string? */
export function isTauriError(err: unknown): err is TauriError {
  return (
    !!err &&
    typeof err === "object" &&
    typeof (err as TauriError).kind === "string" &&
    typeof (err as TauriError).message === "string"
  );
}
