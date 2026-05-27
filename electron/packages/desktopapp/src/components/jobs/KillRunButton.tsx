"use client";

import { Button } from "react-aria-components";
import { Loader2, Square } from "lucide-react";
import { useKillRun } from "@/apis/useKillRun";

/**
 * KillRunButton
 *
 * Icon-only destructive button that sends a kill signal to an in-flight
 * run. Each instance owns its own `useKillRun()` mutation state, so a
 * row-level button on the runs table can spin without affecting siblings.
 * Renders no confirmation dialog — kill is a single click.
 */

export interface KillRunButtonProps {
  runId: string;
  /** Visual size — `sm` (24px hit area) for table rows, `md` (32px) for the log-viewer top bar. Defaults to `md`. */
  size?: "sm" | "md";
}

export function KillRunButton({ runId, size = "md" }: KillRunButtonProps) {
  const { kill, killing } = useKillRun();
  const padding = size === "sm" ? "p-1.5" : "p-2";
  const iconSize = size === "sm" ? 12 : 14;

  return (
    <Button
      aria-label="Kill run"
      isDisabled={killing}
      onPress={() => {
        void kill(runId);
      }}
      className={`inline-flex items-center justify-center ${padding} rounded-input border border-status-failed-border bg-status-failed-bg text-status-failed hover:bg-status-failed/10 cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-brand-ring disabled:opacity-50`}
    >
      {killing ? (
        <Loader2 size={iconSize} className="animate-spin" />
      ) : (
        <Square size={iconSize} />
      )}
    </Button>
  );
}
