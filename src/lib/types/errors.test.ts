import { describe, it, expect } from "vitest";
import { formatError, isTauriError } from "./errors";

describe("formatError", () => {
  it("returns strings unchanged", () => {
    expect(formatError("already a string")).toBe("already a string");
  });

  it("extracts message from a structured TauriError", () => {
    expect(
      formatError({ kind: "AiProvider", message: "rate limit exceeded" })
    ).toBe("rate limit exceeded");
  });

  it("extracts message from a native Error", () => {
    expect(formatError(new Error("boom"))).toBe("boom");
  });

  it("JSON-stringifies objects without a string message field", () => {
    expect(formatError({ code: 42 })).toBe('{"code":42}');
  });

  it("falls back to String() for primitives", () => {
    expect(formatError(null)).toBe("null");
    expect(formatError(undefined)).toBe("undefined");
    expect(formatError(123)).toBe("123");
  });

  it("survives circular references", () => {
    const obj: Record<string, unknown> = { name: "x" };
    obj.self = obj;
    expect(formatError(obj)).toBe("[object Object]");
  });
});

describe("isTauriError", () => {
  it("accepts well-formed structured errors", () => {
    expect(isTauriError({ kind: "Processing", message: "bad WAV" })).toBe(true);
  });

  it("rejects strings", () => {
    expect(isTauriError("error")).toBe(false);
  });

  it("rejects plain Error instances", () => {
    expect(isTauriError(new Error("boom"))).toBe(false);
  });

  it("rejects null/undefined", () => {
    expect(isTauriError(null)).toBe(false);
    expect(isTauriError(undefined)).toBe(false);
  });

  it("rejects partial objects", () => {
    expect(isTauriError({ kind: "Other" })).toBe(false);
    expect(isTauriError({ message: "x" })).toBe(false);
  });
});
