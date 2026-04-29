"use client";

import { useState } from "react";
import { Button } from "react-aria-components";
import { Clock, DatabaseBackup, ArrowRight, X } from "lucide-react";

interface SystemBannerProps {
  version?: string;
  uptime?: string;
  backupsEnabled?: boolean;
  updateAvailable?: string;
  onUpdate?: () => void;
  onDismissUpdate?: () => void;
}

export function SystemBanner({
  version = "3.1.0",
  uptime = "4d 12h 33m",
  backupsEnabled = true,
  updateAvailable,
  onUpdate,
  onDismissUpdate,
}: SystemBannerProps) {
  const [dismissed, setDismissed] = useState(false);
  const showUpdate = updateAvailable && !dismissed;

  return (
    <div className={`flex items-center h-[var(--height-banner)] px-5 bg-surface-secondary border rounded-pill text-sm text-fg-muted ${showUpdate ? "border-brand-ring" : "border-border"}`}>
      {/* Left — uptime & backups */}
      <div className="flex items-center gap-5">
        <span className="inline-flex items-center gap-2 leading-none">
          <Clock size={14} className="text-fg-subtle shrink-0" />
          <span>Uptime</span>
          <span className="text-status-success">{uptime}</span>
        </span>
        <span className="text-fg-ghost">|</span>
        <span className="inline-flex items-center gap-2 leading-none">
          <DatabaseBackup size={14} className="text-fg-subtle shrink-0" />
          <span>Backups</span>
          <span className={backupsEnabled ? "text-status-success" : "text-fg-subtle"}>
            {backupsEnabled ? "Enabled" : "Disabled"}
          </span>
        </span>
      </div>

      {/* Right — version / update pill */}
      <div className="ml-auto">
        {showUpdate ? (
          <span className="inline-flex items-center gap-1.5 leading-none">
            <Button
              onPress={onUpdate}
              className="inline-flex items-center gap-1.5 leading-none bg-brand hover:bg-brand-hover text-surface px-3.5 py-1.5 rounded-pill transition-colors outline-none focus-visible:ring-2 focus-visible:ring-brand-ring"
            >
              Update to v{updateAvailable}
              <ArrowRight size={14} />
            </Button>
            <Button
              onPress={() => { setDismissed(true); onDismissUpdate?.(); }}
              className="text-fg-faint hover:text-fg-muted transition-colors p-1 outline-none focus-visible:ring-2 focus-visible:ring-brand-ring rounded-full"
              aria-label="Dismiss update"
            >
              <X size={14} />
            </Button>
          </span>
        ) : (
          <span className="inline-flex items-center leading-none text-fg-subtle border border-border px-3.5 py-1.5 rounded-pill">
            v{version}
          </span>
        )}
      </div>
    </div>
  );
}
