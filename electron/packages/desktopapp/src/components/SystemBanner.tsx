import { useState } from "react";
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
    <div className={`flex items-center h-12 px-5 bg-gray-50 border rounded-full text-sm text-gray-500 ${showUpdate ? "border-pink-300" : "border-gray-200"}`}>
      {/* Left — uptime & backups */}
      <div className="flex items-center gap-5">
        <span className="inline-flex items-center gap-2 leading-none">
          <Clock size={14} className="text-gray-400 shrink-0" />
          <span>Uptime</span>
          <span className="text-green-600">{uptime}</span>
        </span>
        <span className="text-gray-200">|</span>
        <span className="inline-flex items-center gap-2 leading-none">
          <DatabaseBackup size={14} className="text-gray-400 shrink-0" />
          <span>Backups</span>
          <span className={backupsEnabled ? "text-green-600" : "text-gray-400"}>
            {backupsEnabled ? "Enabled" : "Disabled"}
          </span>
        </span>
      </div>

      {/* Right — version / update pill */}
      <div className="ml-auto">
        {showUpdate ? (
          <span className="inline-flex items-center gap-1.5 leading-none">
            <button
              onClick={onUpdate}
              className="inline-flex items-center gap-1.5 leading-none bg-pink-500 hover:bg-pink-600 text-white px-3.5 py-1.5 rounded-full transition-colors"
            >
              Update to v{updateAvailable}
              <ArrowRight size={14} />
            </button>
            <button
              onClick={() => { setDismissed(true); onDismissUpdate?.(); }}
              className="text-gray-300 hover:text-gray-500 transition-colors p-1"
              title="Dismiss"
            >
              <X size={14} />
            </button>
          </span>
        ) : (
          <span className="inline-flex items-center leading-none text-gray-400 border border-gray-200 px-3.5 py-1.5 rounded-full">
            v{version}
          </span>
        )}
      </div>
    </div>
  );
}
