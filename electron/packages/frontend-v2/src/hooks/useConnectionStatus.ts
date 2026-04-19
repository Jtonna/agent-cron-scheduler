"use client";

import { useState, useEffect, useCallback } from "react";
import { getBaseUrl } from "@/lib/api";

export type ConnectionState = "connected" | "connecting" | "offline";

export function useConnectionStatus(pollIntervalMs = 2500) {
  const [state, setState] = useState<ConnectionState>("connecting");

  const checkHealth = useCallback(async () => {
    try {
      const res = await fetch(`${getBaseUrl()}/health`, {
        signal: AbortSignal.timeout(2000),
      });
      setState(res.ok ? "connected" : "offline");
    } catch {
      setState("offline");
    }
  }, []);

  useEffect(() => {
    checkHealth();
    const interval = setInterval(checkHealth, pollIntervalMs);
    return () => clearInterval(interval);
  }, [checkHealth, pollIntervalMs]);

  return state;
}
