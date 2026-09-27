import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, api, getActiveRunId, getBaseUrl } from "./client";

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

  it("captures details when provided", () => {
    const details = { active_run_id: "r-123" };
    const err = new ApiError(409, "concurrent_run_active", "A run is already active", details);
    expect(err.details).toBe(details);
  });

  it("has undefined details when not provided", () => {
    const err = new ApiError(404, "NOT_FOUND", "Job missing");
    expect(err.details).toBeUndefined();
  });
});

describe("api.deleteWorkflow", () => {
  let mockFetch: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockFetch = vi.fn();
    vi.stubGlobal("fetch", mockFetch);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("resolves to undefined on a 204 with no content-type", async () => {
    mockFetch.mockResolvedValueOnce(
      new Response(null, {
        status: 204,
        statusText: "No Content",
        headers: {},
      })
    );

    const result = await api.deleteWorkflow("test-id");
    expect(result).toBeUndefined();
    expect(mockFetch).toHaveBeenCalledWith(`${getBaseUrl()}/api/workflows/test-id`, {
      method: "DELETE",
      headers: {
        "Content-Type": "application/json",
      },
    });
  });

  it("rejects with ApiError on a 409 with workflow_run_active code", async () => {
    const errorMessage = "Cannot delete workflow 'x' while run y is still running. Kill the run or wait for it to finish, then retry.";
    const makeResponse = () =>
      new Response(
        JSON.stringify({
          error: "workflow_run_active",
          message: errorMessage,
        }),
        {
          status: 409,
          statusText: "Conflict",
          headers: { "content-type": "application/json" },
        }
      );

    mockFetch.mockResolvedValueOnce(makeResponse());
    await expect(api.deleteWorkflow("test-id")).rejects.toBeInstanceOf(ApiError);

    mockFetch.mockResolvedValueOnce(makeResponse());
    await expect(api.deleteWorkflow("test-id")).rejects.toMatchObject({
      status: 409,
      code: "workflow_run_active",
      message: errorMessage,
    });
  });

  it("uses code when both code and error are present", async () => {
    mockFetch.mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          code: "PREFERRED_CODE",
          error: "fallback_error",
          message: "Test message",
        }),
        {
          status: 400,
          statusText: "Bad Request",
          headers: { "content-type": "application/json" },
        }
      )
    );

    await expect(api.deleteWorkflow("test-id")).rejects.toMatchObject({
      code: "PREFERRED_CODE",
    });
  });

  it("parses details from JSON error body for concurrent_run_active", async () => {
    mockFetch.mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          error: "concurrent_run_active",
          message: "A run is already active",
          active_run_id: "r-123",
        }),
        {
          status: 409,
          statusText: "Conflict",
          headers: { "content-type": "application/json" },
        }
      )
    );

    await expect(api.deleteWorkflow("test-id")).rejects.toMatchObject({
      status: 409,
      code: "concurrent_run_active",
      details: {
        error: "concurrent_run_active",
        message: "A run is already active",
        active_run_id: "r-123",
      },
    });
  });

  it("sets details to undefined for non-JSON error body", async () => {
    mockFetch.mockResolvedValueOnce(
      new Response("Internal Server Error", {
        status: 500,
        statusText: "Internal Server Error",
        headers: { "content-type": "text/plain" },
      })
    );

    await expect(api.deleteWorkflow("test-id")).rejects.toMatchObject({
      status: 500,
      code: "UNKNOWN",
      details: undefined,
    });
  });

  it("does not pass array bodies as details", async () => {
    mockFetch.mockResolvedValueOnce(
      new Response(JSON.stringify([{ error: "something" }]), {
        status: 400,
        statusText: "Bad Request",
        headers: { "content-type": "application/json" },
      })
    );

    await expect(api.deleteWorkflow("test-id")).rejects.toMatchObject({
      status: 400,
      details: undefined,
    });
  });
});

describe("getActiveRunId", () => {
  it("returns active_run_id from concurrent_run_active 409 error", () => {
    const err = new ApiError(409, "concurrent_run_active", "A run is already active", {
      active_run_id: "r-123",
    });
    expect(getActiveRunId(err)).toBe("r-123");
  });

  it("returns null for non-ApiError", () => {
    const err = new Error("Generic error");
    expect(getActiveRunId(err)).toBeNull();
  });

  it("returns null for ApiError with non-409 status", () => {
    const err = new ApiError(500, "internal_error", "Server error", {
      active_run_id: "r-123",
    });
    expect(getActiveRunId(err)).toBeNull();
  });

  it("returns null for 409 ApiError with different code", () => {
    const err = new ApiError(409, "workflow_run_active", "Workflow run active", {
      active_run_id: "r-123",
    });
    expect(getActiveRunId(err)).toBeNull();
  });

  it("returns null when active_run_id is missing", () => {
    const err = new ApiError(409, "concurrent_run_active", "A run is already active", {
      message: "A run is already active",
    });
    expect(getActiveRunId(err)).toBeNull();
  });

  it("returns null when active_run_id is not a string", () => {
    const err = new ApiError(409, "concurrent_run_active", "A run is already active", {
      active_run_id: 123,
    });
    expect(getActiveRunId(err)).toBeNull();
  });
});
