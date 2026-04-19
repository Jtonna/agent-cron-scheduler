"use client";

import { useState, useCallback } from "react";
import { useParams, useNavigate } from "react-router-dom";
import {
  ArrowPathIcon,
  PlayIcon,
  TrashIcon,
  CodeBracketIcon,
} from "@heroicons/react/24/outline";
import { toast } from "sonner";
import { SubNavTabs } from "@/components/layout/SubNavTabs";
import { RunTable } from "@/components/RunTable";
import { CostPerRunChart } from "@/components/CostPerRunChart";
import { DeleteJobDialog } from "@/components/DeleteJobDialog";
import { TriggerOverrideDialog } from "@/components/TriggerOverrideDialog";
import { JobForm } from "@/components/jobs/JobForm";
import { ScriptEditor } from "@/components/jobs/ScriptEditor";
import { JobStatusBadge } from "@/lib/job-status";
import { useJob } from "@/hooks/useJob";
import { useRuns } from "@/hooks/useRuns";
import { useCostSummary } from "@/hooks/useCostSummary";
import { useSSEEvents } from "@/hooks/useSSE";
import { api } from "@/lib/api";
import { formatDate } from "@/lib/format";
import { Button } from "@/components/ui/button";
import type { NewJob } from "@/lib/types";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface DynamicTab {
  id: string;
  label: string;
}

type ScriptType = "pre-run" | "main" | "post-run";

const SCRIPT_LABELS: Record<ScriptType, string> = {
  "pre-run": "Pre-Run Script",
  main: "Main Script",
  "post-run": "Post-Run Script",
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function JobDetailPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const jobId = id!;

  // Data hooks
  const { job, loading, error, refresh: refreshJob } = useJob(jobId);
  const {
    runs,
    totalCount,
    loading: runsLoading,
    loadMore,
  } = useRuns(jobId);
  const {
    costSummary,
    loading: costLoading,
    setTimeframe,
  } = useCostSummary(jobId);

  // Tab state
  const [activeTab, setActiveTab] = useState("overview");
  const [dynamicTabs, setDynamicTabs] = useState<DynamicTab[]>([]);

  // Dialog state
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);
  const [showTriggerDialog, setShowTriggerDialog] = useState(false);

  // Script editor state — track edits per script type
  const [scriptValues, setScriptValues] = useState<Record<string, string>>({});

  // SSE: refresh job data on job_changed events
  useSSEEvents(
    useCallback(
      (event) => {
        if (event.type === "job_changed") {
          try {
            const payload = JSON.parse(event.data);
            if (payload?.job_id === jobId || payload?.id === jobId) {
              refreshJob();
            }
          } catch {
            // ignore parse errors
          }
        }
      },
      [jobId, refreshJob],
    ),
  );

  // ---- Tab helpers ----

  const addDynamicTab = useCallback(
    (tabId: string, label: string) => {
      setDynamicTabs((prev) => {
        if (prev.some((t) => t.id === tabId)) return prev;
        return [...prev, { id: tabId, label }];
      });
      setActiveTab(tabId);
    },
    [],
  );

  const handleTabClose = useCallback(
    (tabId: string) => {
      setDynamicTabs((prev) => prev.filter((t) => t.id !== tabId));
      if (activeTab === tabId) {
        setActiveTab("overview");
      }
    },
    [activeTab],
  );

  // ---- Handlers ----

  const handleRunClick = useCallback(
    (runId: string) => {
      navigate(`/jobs/${jobId}/runs/${runId}`);
    },
    [jobId, navigate],
  );

  const handleOpenScript = useCallback(
    (scriptType: ScriptType) => {
      const tabId = `script:${scriptType}`;
      const label = SCRIPT_LABELS[scriptType];

      // Seed editor value from job data if not already edited
      if (!scriptValues[tabId] && job) {
        let value = "";
        switch (scriptType) {
          case "pre-run":
            value = job.pre_hook ?? "";
            break;
          case "main":
            value = job.execution?.value ?? "";
            break;
          case "post-run":
            value = job.post_hook ?? "";
            break;
        }
        setScriptValues((prev) => ({ ...prev, [tabId]: value }));
      }

      addDynamicTab(tabId, label);
    },
    [addDynamicTab, job, scriptValues],
  );

  const handleScriptChange = useCallback(
    (tabId: string, value: string) => {
      setScriptValues((prev) => ({ ...prev, [tabId]: value }));
    },
    [],
  );

  const handleScriptSave = useCallback(
    async (scriptType: ScriptType) => {
      const tabId = `script:${scriptType}`;
      const value = scriptValues[tabId] ?? "";

      try {
        switch (scriptType) {
          case "pre-run":
            await api.updateJob(jobId, { pre_hook: value || null });
            break;
          case "main":
            await api.updateJob(jobId, {
              execution: { type: job?.execution?.type ?? "ShellCommand", value },
            });
            break;
          case "post-run":
            await api.updateJob(jobId, { post_hook: value || null });
            break;
        }
        toast.success(`${SCRIPT_LABELS[scriptType]} saved`);
        refreshJob();
      } catch (err) {
        toast.error(
          `Failed to save script: ${err instanceof Error ? err.message : "Unknown error"}`,
        );
      }
    },
    [jobId, job, scriptValues, refreshJob],
  );

  const handleConfigSubmit = useCallback(
    async (data: NewJob) => {
      try {
        await api.updateJob(jobId, data);
        toast.success("Job updated successfully");
        refreshJob();
      } catch (err) {
        toast.error(
          `Failed to update job: ${err instanceof Error ? err.message : "Unknown error"}`,
        );
        throw err;
      }
    },
    [jobId, refreshJob],
  );

  const handleDeleted = useCallback(() => {
    navigate("/jobs");
  }, [navigate]);

  const handleTriggered = useCallback(
    (runId: string) => {
      navigate(`/jobs/${jobId}/runs/${runId}`);
    },
    [jobId, navigate],
  );

  // Determine the language hint for the script editor
  const getScriptLanguage = useCallback(
    (scriptType: ScriptType): "shell" | "batch" | "python" | "powershell" => {
      if (!job) return "shell";
      if (scriptType === "main") {
        // Infer from execution type — ShellCommand is shell, ScriptFile depends on extension
        const val = job.execution?.value ?? "";
        if (val.endsWith(".py")) return "python";
        if (val.endsWith(".ps1")) return "powershell";
        if (val.endsWith(".bat") || val.endsWith(".cmd")) return "batch";
        return "shell";
      }
      // Hooks have a script_type field
      const hookType =
        scriptType === "pre-run"
          ? job.pre_hook_script_type
          : job.post_hook_script_type;
      if (hookType === "python") return "python";
      if (hookType === "powershell") return "powershell";
      if (hookType === "batch") return "batch";
      return "shell";
    },
    [job],
  );

  // ---- Build tab list ----

  const staticTabs = [
    { id: "overview", label: "Overview" },
    { id: "configuration", label: "Configuration" },
  ];

  const allTabs = [
    ...staticTabs,
    ...dynamicTabs.map((t) => ({ ...t, closable: true })),
  ];

  // ---- Cost chart timeframe ----

  const costTimeframe = costSummary?.timeframe ?? "30d";

  // ---- Loading state ----

  if (loading && !job) {
    return (
      <div className="flex items-center justify-center py-12">
        <ArrowPathIcon className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  // ---- Error state ----

  if (error && !job) {
    return (
      <div className="flex flex-col gap-4 p-6">
        <div className="rounded-lg border border-destructive/50 bg-destructive/10 p-4">
          <p className="text-sm text-destructive">{error}</p>
        </div>
        <Button variant="outline" size="sm" onClick={() => navigate("/jobs")}>
          Back to Jobs
        </Button>
      </div>
    );
  }

  // ---- Render ----

  return (
    <div className="flex flex-col h-full">
      {/* Tab bar */}
      <SubNavTabs
        tabs={allTabs}
        activeTab={activeTab}
        onTabChange={setActiveTab}
        onTabClose={handleTabClose}
      />

      {/* Tab content */}
      <div className="flex-1 overflow-y-auto">
        {/* ---- OVERVIEW TAB ---- */}
        {activeTab === "overview" && job && (
          <div className="flex flex-col gap-6 p-6">
            {/* Job header */}
            <div className="flex flex-col gap-2">
              <div className="flex items-center gap-3 flex-wrap">
                <h1 className="text-xl font-semibold">{job.name}</h1>
                <JobStatusBadge
                  enabled={job.enabled}
                  last_exit_code={job.last_exit_code}
                  last_run_at={job.last_run_at}
                />
              </div>
              <div className="flex items-center gap-4 text-sm text-muted-foreground">
                <span>
                  Schedule:{" "}
                  <span className="font-mono text-foreground">
                    {job.schedule}
                  </span>
                </span>
                {job.next_run_at && (
                  <span>
                    Next run:{" "}
                    <span className="text-foreground">
                      {formatDate(job.next_run_at)}
                    </span>
                  </span>
                )}
              </div>
            </div>

            {/* Action buttons */}
            <div className="flex items-center gap-2">
              <Button
                size="sm"
                className="gap-1.5"
                onClick={() => setShowTriggerDialog(true)}
              >
                <PlayIcon className="h-4 w-4" />
                Trigger Now
              </Button>
              <Button
                size="sm"
                variant="outline"
                className="gap-1.5"
                onClick={() => handleOpenScript("main")}
              >
                <CodeBracketIcon className="h-4 w-4" />
                View Script
              </Button>
              {job.pre_hook && (
                <Button
                  size="sm"
                  variant="outline"
                  className="gap-1.5"
                  onClick={() => handleOpenScript("pre-run")}
                >
                  <CodeBracketIcon className="h-4 w-4" />
                  Pre-Run
                </Button>
              )}
              {job.post_hook && (
                <Button
                  size="sm"
                  variant="outline"
                  className="gap-1.5"
                  onClick={() => handleOpenScript("post-run")}
                >
                  <CodeBracketIcon className="h-4 w-4" />
                  Post-Run
                </Button>
              )}
              <div className="flex-1" />
              <Button
                size="sm"
                variant="destructive"
                className="gap-1.5"
                onClick={() => setShowDeleteDialog(true)}
              >
                <TrashIcon className="h-4 w-4" />
                Delete
              </Button>
            </div>

            {/* Cost chart section */}
            <div className="flex flex-col gap-2">
              <h2 className="text-sm font-semibold">Cost per Run</h2>
              <CostPerRunChart
                data={costSummary?.data ?? []}
                loading={costLoading}
                timeframe={costTimeframe}
                onTimeframeChange={setTimeframe}
              />
            </div>

            {/* Run history section */}
            <div className="flex flex-col gap-2">
              <h2 className="text-sm font-semibold">Run History</h2>
              <RunTable
                runs={runs}
                totalCount={totalCount}
                loading={runsLoading}
                hasMore={runs.length < totalCount}
                onLoadMore={loadMore}
                onRunClick={handleRunClick}
              />
            </div>
          </div>
        )}

        {/* ---- CONFIGURATION TAB ---- */}
        {activeTab === "configuration" && job && (
          <div className="w-full">
            <JobForm
              job={job}
              title={`Edit: ${job.name}`}
              onSubmit={handleConfigSubmit}
              submitLabel="Update Job"
            />
          </div>
        )}

        {/* ---- SCRIPT EDITOR DYNAMIC TABS ---- */}
        {activeTab.startsWith("script:") && job && (() => {
          const scriptType = activeTab.replace("script:", "") as ScriptType;
          const tabId = activeTab;
          const value = scriptValues[tabId] ?? "";
          const language = getScriptLanguage(scriptType);

          return (
            <div className="flex flex-col gap-4 p-6">
              <div className="flex items-center justify-between">
                <h2 className="text-sm font-semibold">
                  {SCRIPT_LABELS[scriptType]}
                </h2>
                <Button
                  size="sm"
                  onClick={() => handleScriptSave(scriptType)}
                >
                  Save
                </Button>
              </div>
              <ScriptEditor
                value={value}
                onChange={(v) => handleScriptChange(tabId, v)}
                language={language}
              />
            </div>
          );
        })()}
      </div>

      {/* Dialogs */}
      {job && (
        <>
          <DeleteJobDialog
            open={showDeleteDialog}
            onOpenChange={setShowDeleteDialog}
            jobId={jobId}
            jobName={job.name}
            onDeleted={handleDeleted}
          />
          <TriggerOverrideDialog
            open={showTriggerDialog}
            onOpenChange={setShowTriggerDialog}
            jobId={jobId}
            jobName={job.name}
            onTriggered={handleTriggered}
          />
        </>
      )}
    </div>
  );
}
