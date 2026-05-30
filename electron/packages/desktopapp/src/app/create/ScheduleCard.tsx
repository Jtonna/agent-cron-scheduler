"use client";

/**
 * ScheduleCard
 *
 * Compact floating card at the top-centre of the canvas. Holds:
 *   - the cron pill (mono input + live cron-to-English preview)
 *   - a timezone pill (popover with a searchable list)
 *   - an enabled toggle
 *   - the primary Save / Create button (label varies by mode)
 *
 * Kept under ~64px tall so it doesn't dominate the canvas.
 */

import { useEffect, useRef, useState } from "react";
import { Clock, Globe2 } from "lucide-react";
import { Toggle } from "@/components/ui/Toggle";
import { cronToEnglish } from "./cron-english";
import { COMMON_TIMEZONES } from "./timezones";
import type { NewWorkflow } from "./types";

export interface ScheduleCardProps {
  workflow: NewWorkflow;
  onChange: (next: NewWorkflow) => void;
  /** Label for the primary submit button (e.g. "Create Workflow", "Save Changes"). */
  submitLabel: string;
  onSubmit: () => void;
  submitDisabled?: boolean;
}

export function ScheduleCard({
  workflow,
  onChange,
  submitLabel,
  onSubmit,
  submitDisabled = false,
}: ScheduleCardProps) {
  const english = cronToEnglish(workflow.schedule);

  return (
    <div className="inline-flex items-center gap-2 bg-surface border border-border rounded-pill shadow-sm pl-2 pr-1 py-1">
      <div className="inline-flex items-center gap-1.5 px-2">
        <Clock size={13} className="text-fg-muted" aria-hidden />
        <input
          type="text"
          value={workflow.schedule}
          onChange={(e) => onChange({ ...workflow, schedule: e.target.value })}
          placeholder="0 9 * * *"
          className="font-mono text-[12px] text-fg bg-transparent outline-none w-[110px] focus:bg-surface-hover rounded-input px-1"
          aria-label="Cron schedule"
        />
        <span className="text-[11px] text-fg-muted whitespace-nowrap pl-1 border-l border-border-subtle pl-2">
          {english}
        </span>
      </div>

      <span className="h-5 w-px bg-border" aria-hidden />

      <TimezonePill
        value={workflow.timezone ?? ""}
        onChange={(tz) =>
          onChange({ ...workflow, timezone: tz.length > 0 ? tz : undefined })
        }
      />

      <span className="h-5 w-px bg-border" aria-hidden />

      <div className="inline-flex items-center gap-1.5 px-2">
        <Toggle
          size="sm"
          checked={workflow.enabled ?? true}
          onChange={(v) => onChange({ ...workflow, enabled: v })}
          ariaLabel="Enabled"
        />
        <span className="text-[11px] text-fg-muted">enabled</span>
      </div>

      <button
        type="button"
        onClick={onSubmit}
        disabled={submitDisabled}
        className="ml-1 bg-brand hover:bg-brand-hover disabled:opacity-50 disabled:cursor-not-allowed text-surface text-[12px] font-semibold px-3.5 py-1.5 rounded-pill cursor-pointer transition-colors"
      >
        {submitLabel}
      </button>
    </div>
  );
}

interface TimezonePillProps {
  value: string;
  onChange: (next: string) => void;
}

function TimezonePill({ value, onChange }: TimezonePillProps) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function onDown(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const matches = COMMON_TIMEZONES.filter((z) =>
    z.toLowerCase().includes(search.toLowerCase()),
  );

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-pill hover:bg-surface-hover cursor-pointer"
        aria-haspopup="listbox"
        aria-expanded={open}
      >
        <Globe2 size={13} className="text-fg-muted" aria-hidden />
        <span className="text-[11.5px] text-fg max-w-[140px] truncate">
          {value || <span className="text-fg-subtle">no tz</span>}
        </span>
      </button>
      {open && (
        <div className="absolute top-full mt-1.5 right-0 z-30 w-[240px] bg-surface border border-border rounded-card shadow-menu">
          <input
            autoFocus
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search timezones…"
            className="w-full px-2.5 py-2 text-[12px] bg-transparent outline-none border-b border-border text-fg placeholder:text-fg-subtle"
          />
          <ul role="listbox" className="max-h-[220px] overflow-y-auto py-1">
            {value && !COMMON_TIMEZONES.includes(value) && (
              <li
                role="option"
                aria-selected={true}
                className="px-2.5 py-1.5 text-[12px] font-mono text-fg bg-surface-hover cursor-default"
              >
                {value} <span className="text-fg-muted">(current)</span>
              </li>
            )}
            {matches.map((tz) => (
              <li key={tz}>
                <button
                  type="button"
                  role="option"
                  aria-selected={tz === value}
                  onClick={() => {
                    onChange(tz);
                    setOpen(false);
                    setSearch("");
                  }}
                  className={[
                    "w-full text-left px-2.5 py-1.5 text-[12px] font-mono cursor-pointer",
                    tz === value
                      ? "bg-brand-muted text-brand-hover"
                      : "text-fg hover:bg-surface-hover",
                  ].join(" ")}
                >
                  {tz}
                </button>
              </li>
            ))}
            {matches.length === 0 && (
              <li className="px-2.5 py-2 text-[11px] text-fg-muted italic">
                No matches in common list — paste any IANA zone in the cron field&apos;s tz prop.
              </li>
            )}
          </ul>
        </div>
      )}
    </div>
  );
}
