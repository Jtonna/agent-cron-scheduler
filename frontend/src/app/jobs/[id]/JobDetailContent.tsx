"use client";

import React, { useState, useCallback } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { Column, Row, Text, Grid } from "@once-ui-system/core";
import { useJob } from "@/hooks/useJob";
import { useRuns } from "@/hooks/useRuns";
import { Badge, statusToBadgeVariant } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Card } from "@/components/ui/Card";
import { Modal } from "@/components/ui/Modal";
import { Spinner } from "@/components/ui/Spinner";
import { EmptyState } from "@/components/ui/EmptyState";
import { LogViewer } from "@/components/logs/LogViewer";
import { useToast } from "@/components/ui/Toast";
import { useSSEEvents } from "@/hooks/useSSE";
import { api } from "@/lib/api";
import { formatDate, formatBytes } from "@/lib/format";

export function JobDetailContent() {
  const params = useParams();
  const id = params.id as string;
  const { job, loading: jobLoading, error: jobError, refresh: refreshJob } = useJob(id);
  const { runs, total, loading: runsLoading, refresh: refreshRuns } = useRuns(id);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [showDelete, setShowDelete] = useState(false);
  const [actionLoading, setActionLoading] = useState(false);
  const { addToast } = useToast();

  useSSEEvents(
    useCallback(
      (event) => {
        if (
          event.type === "job_changed" ||
          event.type === "completed" ||
          event.type === "failed" ||
          event.type === "started"
        ) {
          refreshJob();
          refreshRuns();
        }
      },
      [refreshJob, refreshRuns]
    )
  );

  const handleTrigger = async () => {
    setActionLoading(true);
    try {
      const result = await api.triggerJob(id);
      addToast("Job triggered", "success");
      setSelectedRunId(result.run_id);
      refreshRuns();
    } catch (err) {
      addToast(
        `Failed to trigger: ${err instanceof Error ? err.message : "Unknown error"}`,
        "error"
      );
    } finally {
      setActionLoading(false);
    }
  };

  const handleDelete = async () => {
    setActionLoading(true);
    try {
      await api.deleteJob(id);
      addToast("Job deleted", "success");
      window.location.href = "/jobs";
    } catch (err) {
      addToast(
        `Failed to delete: ${err instanceof Error ? err.message : "Unknown error"}`,
        "error"
      );
    } finally {
      setActionLoading(false);
      setShowDelete(false);
    }
  };

  if (jobLoading) {
    return (
      <Row horizontal="center" paddingY="48">
        <Spinner size="lg" />
      </Row>
    );
  }

  if (jobError || !job) {
    return (
      <Column
        padding="16"
        radius="l"
        style={{
          background: "var(--danger-alpha-weak)",
          border: "1px solid var(--danger-border-medium)",
        }}
      >
        <Text variant="body-default-s" onBackground="danger-strong">
          {jobError || "Job not found"}
        </Text>
      </Column>
    );
  }

  function getJobStatusBadge() {
    if (!job) return null;
    if (!job.enabled) return <Badge variant="disabled">Disabled</Badge>;
    if (job.last_exit_code === null && !job.last_run_at) return <Badge variant="default">Pending</Badge>;
    if (job.last_exit_code === 0) return <Badge variant="success">OK</Badge>;
    if (job.last_exit_code !== null) return <Badge variant="error">Failed (exit {job.last_exit_code})</Badge>;
    return <Badge variant="default">Unknown</Badge>;
  }

  return (
    <Column gap="24" fillWidth>
      {/* Header */}
      <Row horizontal="between" vertical="start" fillWidth wrap>
        <Column gap="4">
          <Row gap="12" vertical="center">
            <Text variant="heading-strong-l">{job.name}</Text>
            {getJobStatusBadge()}
          </Row>
          <Text
            variant="body-default-s"
            onBackground="neutral-weak"
            style={{ fontFamily: "var(--font-code)" }}
          >
            {job.schedule}
          </Text>
        </Column>
        <Row gap="8">
          <Link href={`/jobs/${id}/edit`}>
            <Button variant="secondary" size="sm">
              Edit
            </Button>
          </Link>
          <Button
            variant="primary"
            size="sm"
            onClick={handleTrigger}
            disabled={actionLoading}
          >
            Trigger
          </Button>
          <Button
            variant="danger"
            size="sm"
            onClick={() => setShowDelete(true)}
          >
            Delete
          </Button>
        </Row>
      </Row>

      {/* Job Config */}
      <Card title="Configuration">
        <Grid columns="2" gap="16" fillWidth>
          <Column gap="2">
            <Text variant="label-default-xs" onBackground="neutral-weak">Type</Text>
            <Text variant="body-default-s">
              {job.execution.type === "ShellCommand" ? "Shell Command" : "Script File"}
            </Text>
          </Column>
          <Column gap="2">
            <Text variant="label-default-xs" onBackground="neutral-weak">Enabled</Text>
            <Text variant="body-default-s">{job.enabled ? "Yes" : "No"}</Text>
          </Column>
          <Column gap="2" style={{ gridColumn: "1 / -1" }}>
            <Text variant="label-default-xs" onBackground="neutral-weak">Command</Text>
            <code
              style={{
                fontFamily: "var(--font-code)",
                fontSize: "var(--font-size-body-xs)",
                background: "var(--neutral-alpha-weak)",
                padding: "4px 8px",
                borderRadius: "var(--radius-s)",
              }}
            >
              {job.execution.value}
            </code>
          </Column>
          {job.timezone && (
            <Column gap="2">
              <Text variant="label-default-xs" onBackground="neutral-weak">Timezone</Text>
              <Text variant="body-default-s">{job.timezone}</Text>
            </Column>
          )}
          {job.working_dir && (
            <Column gap="2">
              <Text variant="label-default-xs" onBackground="neutral-weak">Working Dir</Text>
              <Text
                variant="body-default-xs"
                style={{ fontFamily: "var(--font-code)" }}
              >
                {job.working_dir}
              </Text>
            </Column>
          )}
          <Column gap="2">
            <Text variant="label-default-xs" onBackground="neutral-weak">Timeout</Text>
            <Text variant="body-default-s">{job.timeout_secs}s</Text>
          </Column>
          <Column gap="2">
            <Text variant="label-default-xs" onBackground="neutral-weak">Log Environment</Text>
            <Text variant="body-default-s">{job.log_environment ? "Yes" : "No"}</Text>
          </Column>
          {job.env_vars && Object.keys(job.env_vars).length > 0 && (
            <Column gap="4" style={{ gridColumn: "1 / -1" }}>
              <Text variant="label-default-xs" onBackground="neutral-weak">Environment Variables</Text>
              <Column
                padding="8"
                radius="s"
                gap="2"
                style={{
                  background: "var(--neutral-alpha-weak)",
                  fontFamily: "var(--font-code)",
                  fontSize: "var(--font-size-body-xs)",
                }}
              >
                {Object.entries(job.env_vars).map(([k, v]) => (
                  <Row key={k}>
                    <Text style={{ color: "var(--brand-on-background-strong)", fontFamily: "inherit", fontSize: "inherit" }}>
                      {k}
                    </Text>
                    <Text style={{ fontFamily: "inherit", fontSize: "inherit" }}>=</Text>
                    <Text style={{ fontFamily: "inherit", fontSize: "inherit" }}>{v}</Text>
                  </Row>
                ))}
              </Column>
            </Column>
          )}
        </Grid>
      </Card>

      {/* Run History */}
      <Card title={`Run History (${total})`}>
        {runsLoading && runs.length === 0 ? (
          <Row horizontal="center" paddingY="24">
            <Spinner />
          </Row>
        ) : runs.length === 0 ? (
          <EmptyState message="No runs yet" description="Trigger the job or wait for it to run on schedule." />
        ) : (
          <div style={{ overflowX: "auto" }}>
            <table className="data-table">
              <thead>
                <tr>
                  <th>Status</th>
                  <th>Started</th>
                  <th>Finished</th>
                  <th>Exit</th>
                  <th>Log Size</th>
                </tr>
              </thead>
              <tbody>
                {runs.map((run) => (
                  <tr
                    key={run.run_id}
                    onClick={() => setSelectedRunId(run.run_id)}
                    className={selectedRunId === run.run_id ? "selected" : ""}
                    style={{ cursor: "pointer" }}
                  >
                    <td>
                      <Badge variant={statusToBadgeVariant(run.status)}>
                        {run.status}
                      </Badge>
                    </td>
                    <td>{formatDate(run.started_at)}</td>
                    <td>{formatDate(run.finished_at)}</td>
                    <td style={{ fontFamily: "var(--font-code)" }}>
                      {run.exit_code ?? "--"}
                    </td>
                    <td>{formatBytes(run.log_size_bytes)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>

      {/* Log Viewer */}
      <Card title="Run Log">
        <LogViewer runId={selectedRunId} jobId={id} />
      </Card>

      {/* Delete Modal */}
      <Modal
        open={showDelete}
        onClose={() => setShowDelete(false)}
        title="Delete Job"
        actions={
          <>
            <Button variant="secondary" size="sm" onClick={() => setShowDelete(false)}>
              Cancel
            </Button>
            <Button variant="danger" size="sm" onClick={handleDelete} disabled={actionLoading}>
              {actionLoading ? "Deleting..." : "Delete"}
            </Button>
          </>
        }
      >
        <Text variant="body-default-s" onBackground="neutral-medium">
          Are you sure you want to delete <strong>{job.name}</strong>? This will
          remove the job and all its run history.
        </Text>
      </Modal>
    </Column>
  );
}
