"use client";

import { useState, useEffect, useCallback } from "react";
import type { HealthResponse } from "@/lib/types";

export interface InstanceHealthStatus {
  reachable: boolean;
  version: string | null;
  loading: boolean;
  error: string | null;
}

export interface InstanceHealthCheckResult {
  reachable: boolean;
  version: string | null;
  error: string | null;
}

export interface VersionCompatibilityResult {
  compatible: boolean;
  message: string;
}

const HEALTH_CHECK_TIMEOUT = 3000; // 3 seconds
const HEALTH_POLL_INTERVAL = 10000; // 10 seconds

/**
 * Standalone async function to check the health of a remote ACS instance
 * Fetches {url}/health with a 3-second timeout
 * Returns reachable status, version, and any errors
 */
export async function checkInstanceHealth(
  url: string
): Promise<InstanceHealthCheckResult> {
  try {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), HEALTH_CHECK_TIMEOUT);

    const response = await fetch(`${url}/health`, {
      method: "GET",
      signal: controller.signal,
    });

    clearTimeout(timeoutId);

    if (!response.ok) {
      return {
        reachable: false,
        version: null,
        error: `HTTP ${response.status}: ${response.statusText}`,
      };
    }

    const data = (await response.json()) as HealthResponse;
    return {
      reachable: true,
      version: data.version || null,
      error: null,
    };
  } catch (err) {
    const errorMessage =
      err instanceof Error ? err.message : "Failed to connect to instance";
    return {
      reachable: false,
      version: null,
      error: errorMessage,
    };
  }
}

/**
 * Helper function to compare major versions
 * Returns compatibility status and message
 * Compatible if major versions (first segment before dot) match
 */
export function checkVersionCompatibility(
  localVersion: string,
  remoteVersion: string
): VersionCompatibilityResult {
  try {
    const localMajor = localVersion.split(".")[0];
    const remoteMajor = remoteVersion.split(".")[0];

    const compatible = localMajor === remoteMajor;

    if (compatible) {
      return {
        compatible: true,
        message: `Version compatible (local: v${localVersion}, remote: v${remoteVersion})`,
      };
    }

    return {
      compatible: false,
      message: `Major version mismatch: local v${localMajor} vs remote v${remoteMajor}`,
    };
  } catch (err) {
    return {
      compatible: false,
      message: "Failed to compare versions",
    };
  }
}

/**
 * Hook to poll the health of a remote ACS instance
 * When enabled is true and url is provided, polls {url}/health every 10 seconds
 * Uses a 3-second timeout for each fetch request
 * Returns reachable status, version, loading state, and any errors
 * Automatically cleans up interval on unmount or when disabled
 */
export function useInstanceHealth(
  url: string | null,
  enabled: boolean = true
): InstanceHealthStatus {
  const [reachable, setReachable] = useState(false);
  const [version, setVersion] = useState<string | null>(null);
  const [loading, setLoading] = useState(enabled && !!url);
  const [error, setError] = useState<string | null>(null);

  const checkHealth = useCallback(async () => {
    if (!url || !enabled) {
      return;
    }

    setLoading(true);
    const result = await checkInstanceHealth(url);

    setReachable(result.reachable);
    setVersion(result.version);
    setError(result.error);
    setLoading(false);
  }, [url, enabled]);

  useEffect(() => {
    if (!url || !enabled) {
      setLoading(false);
      setReachable(false);
      setVersion(null);
      setError(null);
      return;
    }

    // Initial check
    checkHealth();

    // Set up interval for polling
    const timer = setInterval(checkHealth, HEALTH_POLL_INTERVAL);

    return () => {
      clearInterval(timer);
    };
  }, [url, enabled, checkHealth]);

  return {
    reachable,
    version,
    loading,
    error,
  };
}
