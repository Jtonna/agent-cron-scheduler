"use client";

import { useState, useEffect, useCallback } from "react";

export type ConnectionState = "connected" | "connecting" | "offline";

export function useConnectionStatus(pollIntervalMs = 2500) {
  const [state, setState] = useState<ConnectionState>("connecting");

  const checkHealth = useCallback(async () => {
    try {
      // Use window.acs if available (Electron), otherwise use relative URL
      const baseUrl =
        (typeof window !== "undefined" && (window as any).acs?.getDaemonUrl?.()) ?? "";
      const res = await fetch(`${baseUrl}/health`, {
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
