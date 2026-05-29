"use client";

/**
 * WorkflowHeaderCard
 *
 * The top card on the /create page that captures workflow-level
 * metadata: name, schedule (cron), timezone, and the enabled toggle.
 * Kept deliberately compact so the bulk of the page can be devoted to
 * the step graph below.
 */

import { Toggle } from "@/components/ui/Toggle";
import type { NewWorkflow } from "./types";

interface WorkflowHeaderCardProps {
  workflow: NewWorkflow;
  onChange: (next: NewWorkflow) => void;
}

export function WorkflowHeaderCard({ workflow, onChange }: WorkflowHeaderCardProps) {
  return (
    <div className="bg-surface border border-border rounded-card p-5 space-y-4">
      <div className="grid grid-cols-1 md:grid-cols-[1.5fr_1fr_1fr_auto] gap-4 items-end">
        <div>
          <label className="text-eyebrow !text-fg-tertiary block mb-1.5">Workflow name</label>
          <input
            type="text"
            value={workflow.name}
            onChange={(e) => onChange({ ...workflow, name: e.target.value })}
            placeholder="fresno-weather-summary"
            className="w-full px-2.5 py-1.5 text-sm bg-surface border border-border rounded-input text-fg placeholder:text-fg-subtle focus:outline-none focus:border-fg"
          />
        </div>
        <div>
          <label className="text-eyebrow !text-fg-tertiary block mb-1.5">
            Schedule (cron)
          </label>
          <input
            type="text"
            value={workflow.schedule}
            onChange={(e) => onChange({ ...workflow, schedule: e.target.value })}
            placeholder="0 9 * * *"
            className="w-full px-2.5 py-1.5 text-sm font-mono bg-surface border border-border rounded-input text-fg placeholder:text-fg-subtle focus:outline-none focus:border-fg"
          />
          <div className="text-[10px] text-fg-subtle mt-1 font-mono">
            5/6-field cron. e.g. `0 9 * * *` for 9am daily.
          </div>
        </div>
        <div>
          <label className="text-eyebrow !text-fg-tertiary block mb-1.5">Timezone</label>
          <input
            type="text"
            value={workflow.timezone ?? ""}
            onChange={(e) =>
              onChange({
                ...workflow,
                timezone: e.target.value.length ? e.target.value : undefined,
              })
            }
            placeholder="America/Los_Angeles"
            className="w-full px-2.5 py-1.5 text-sm bg-surface border border-border rounded-input text-fg placeholder:text-fg-subtle focus:outline-none focus:border-fg"
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <span className="text-eyebrow !text-fg-tertiary">Enabled</span>
          <div className="h-[34px] flex items-center">
            <Toggle
              size="md"
              checked={workflow.enabled ?? true}
              onChange={(v) => onChange({ ...workflow, enabled: v })}
              ariaLabel="Enabled"
            />
          </div>
        </div>
      </div>
    </div>
  );
}
