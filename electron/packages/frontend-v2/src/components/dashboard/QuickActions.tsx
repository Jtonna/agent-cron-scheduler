"use client";

import { useNavigate } from "react-router-dom";
import {
  QueueListIcon,
  PlusCircleIcon,
  ArrowPathIcon,
  CommandLineIcon,
} from "@heroicons/react/24/outline";
import { api } from "@/lib/api";

export function QuickActions({ scrollToLogs }: { scrollToLogs?: () => void }) {
  const navigate = useNavigate();

  const handleRestart = async () => {
    const confirmed = window.confirm(
      "Are you sure you want to restart the server?"
    );
    if (!confirmed) return;
    await api.restart();
  };

  return (
    <div className="flex flex-wrap gap-2">
      <button
        type="button"
        onClick={() => navigate("/jobs")}
        className="inline-flex items-center gap-2 rounded-full border border-border bg-card px-4 py-2 text-sm text-foreground transition-colors hover:bg-muted"
      >
        <QueueListIcon className="size-4 text-muted-foreground" />
        Jobs
      </button>

      <button
        type="button"
        onClick={() => navigate("/jobs/create")}
        className="inline-flex items-center gap-2 rounded-full border border-border bg-card px-4 py-2 text-sm text-foreground transition-colors hover:bg-muted"
      >
        <PlusCircleIcon className="size-4 text-muted-foreground" />
        Create Job
      </button>

      <button
        type="button"
        onClick={handleRestart}
        className="inline-flex items-center gap-2 rounded-full border border-border bg-card px-4 py-2 text-sm text-foreground transition-colors hover:bg-muted"
      >
        <ArrowPathIcon className="size-4 text-muted-foreground" />
        Restart
      </button>

      <button
        type="button"
        onClick={() => scrollToLogs?.()}
        className="inline-flex items-center gap-2 rounded-full border border-border bg-card px-4 py-2 text-sm text-foreground transition-colors hover:bg-muted"
      >
        <CommandLineIcon className="size-4 text-muted-foreground" />
        System Logs
      </button>
    </div>
  );
}
