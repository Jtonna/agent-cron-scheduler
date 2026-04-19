"use client";

import type { HealthResponse } from "@/lib/types";
import { formatUptime } from "@/lib/format";

interface FooterProps {
  health: HealthResponse | null;
}

export function Footer({ health }: FooterProps) {
  return (
    <footer className="fixed bottom-0 left-0 right-0 z-40 border-t border-border/40 bg-card px-6 py-2">
      <div className="flex items-center gap-3 text-[11px] font-mono text-muted-foreground">
        {/* Status dot */}
        <span className="flex items-center gap-1.5" aria-label="system status">
          <span
            className={[
              "inline-block size-1.5 rounded-full shrink-0",
              health?.status === "ok"
                ? "bg-success animate-pulse"
                : "bg-destructive",
            ].join(" ")}
            data-testid="status-dot"
          />
          <span>{health?.status === "ok" ? "ok" : health ? health.status : "—"}</span>
        </span>

        <span className="text-border/60">·</span>

        {/* Uptime */}
        <span>
          {health != null
            ? `up ${formatUptime(health.uptime_seconds)}`
            : "up —"}
        </span>

        <span className="text-border/60">·</span>

        {/* Active / Total jobs */}
        <span>
          {health != null
            ? `${health.active_jobs} / ${health.total_jobs} jobs active`
            : "— / — jobs active"}
        </span>

        <span className="text-border/60">·</span>

        {/* Version */}
        <span>v{health?.version ?? "—"}</span>

        <span className="text-border/60">·</span>

        {/* Data directory */}
        <span className="truncate max-w-[320px]" title={health?.data_dir}>
          {health?.data_dir ?? "—"}
        </span>
      </div>
    </footer>
  );
}
