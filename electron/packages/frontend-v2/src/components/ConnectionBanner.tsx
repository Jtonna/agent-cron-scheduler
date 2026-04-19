"use client";

import { useConnectionStatus } from "@/hooks/useConnectionStatus";

export function ConnectionBanner() {
  const status = useConnectionStatus();

  if (status === "connected") return null;

  const isConnecting = status === "connecting";

  return (
    <div
      className={`sticky top-0 z-50 px-4 py-2 text-center text-sm font-medium ${
        isConnecting
          ? "bg-amber-500/90 text-amber-950 animate-pulse"
          : "bg-red-500/90 text-white"
      }`}
    >
      {isConnecting
        ? "Connecting to ACS daemon..."
        : "ACS daemon is offline — reconnecting..."}
    </div>
  );
}
