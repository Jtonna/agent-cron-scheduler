import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, getBaseUrl } from "./client";

describe("getBaseUrl", () => {
  const ORIGINAL_ENV = process.env.NEXT_PUBLIC_API_URL;
  let originalAcsApiUrl: unknown;
  const acsKey = "__ACS_API_URL__" as const;

  beforeEach(() => {
    originalAcsApiUrl = (window as unknown as Record<string, unknown>)[acsKey];
  });

  afterEach(() => {
    if (ORIGINAL_ENV === undefined) {
      delete process.env.NEXT_PUBLIC_API_URL;
    } else {
      process.env.NEXT_PUBLIC_API_URL = ORIGINAL_ENV;
    }
    if (originalAcsApiUrl === undefined) {
      delete (window as unknown as Record<string, unknown>)[acsKey];
    } else {
      (window as unknown as Record<string, unknown>)[acsKey] = originalAcsApiUrl;
    }
    vi.unstubAllEnvs();
  });

  it("returns the default fallback when nothing is configured", () => {
    delete process.env.NEXT_PUBLIC_API_URL;
    delete (window as unknown as Record<string, unknown>)[acsKey];
    expect(getBaseUrl()).toBe("http://127.0.0.1:8377");
  });

  it("prefers NEXT_PUBLIC_API_URL over the default", () => {
    delete (window as unknown as Record<string, unknown>)[acsKey];
    vi.stubEnv("NEXT_PUBLIC_API_URL", "http://example.test:1234");
    expect(getBaseUrl()).toBe("http://example.test:1234");
  });

  it("prefers window.__ACS_API_URL__ over the env var", () => {
    vi.stubEnv("NEXT_PUBLIC_API_URL", "http://example.test:1234");
    (window as unknown as Record<string, unknown>)[acsKey] = "http://injected.local:9999";
    expect(getBaseUrl()).toBe("http://injected.local:9999");
  });
});

describe("ApiError", () => {
  it("captures status, code, and message", () => {
    const err = new ApiError(404, "NOT_FOUND", "Job missing");
    expect(err).toBeInstanceOf(Error);
    expect(err).toBeInstanceOf(ApiError);
    expect(err.name).toBe("ApiError");
    expect(err.status).toBe(404);
    expect(err.code).toBe("NOT_FOUND");
    expect(err.message).toBe("Job missing");
  });
});
