"use client";

import React, { useState } from "react";
import Link from "next/link";
import { Row, Text, Icon, IconButton } from "@once-ui-system/core";
import type { Job } from "@/lib/types";
import { Badge, statusToBadgeVariant } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { Toggle } from "@/components/ui/Toggle";
import { Modal } from "@/components/ui/Modal";
import { EmptyState } from "@/components/ui/EmptyState";
import { formatDate } from "@/lib/format";

type SortField = "name" | "last_run_at";
type SortDir = "asc" | "desc";

interface JobTableProps {
  jobs: Job[];
  onTrigger: (id: string) => Promise<void>;
  onEnable: (id: string) => Promise<void>;
  onDisable: (id: string) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
}

export function JobTable({
  jobs,
  onTrigger,
  onEnable,
  onDisable,
  onDelete,
}: JobTableProps) {
  const [sortField, setSortField] = useState<SortField>("name");
  const [sortDir, setSortDir] = useState<SortDir>("asc");
  const [deleteTarget, setDeleteTarget] = useState<Job | null>(null);
  const [actionLoading, setActionLoading] = useState<string | null>(null);

  if (jobs.length === 0) {
    return (
      <EmptyState
        message="No jobs found"
        description="Create your first job to get started."
      />
    );
  }

  const sorted = [...jobs].sort((a, b) => {
    let cmp = 0;
    if (sortField === "name") {
      cmp = a.name.localeCompare(b.name);
    } else if (sortField === "last_run_at") {
      const aTime = a.last_run_at ? new Date(a.last_run_at).getTime() : 0;
      const bTime = b.last_run_at ? new Date(b.last_run_at).getTime() : 0;
      cmp = aTime - bTime;
    }
    return sortDir === "asc" ? cmp : -cmp;
  });

  const toggleSort = (field: SortField) => {
    if (sortField === field) {
      setSortDir(sortDir === "asc" ? "desc" : "asc");
    } else {
      setSortField(field);
      setSortDir("asc");
    }
  };

  const SortIndicator = ({ field }: { field: SortField }) => (
    <span style={{ marginLeft: "4px", opacity: sortField === field ? 1 : 0.4 }}>
      {sortField === field && sortDir === "desc" ? "\u25B2" : "\u25BC"}
    </span>
  );

  const handleAction = async (id: string, action: () => Promise<void>) => {
    setActionLoading(id);
    try {
      await action();
    } catch {
      // Error handled by parent
    } finally {
      setActionLoading(null);
    }
  };

  const handleDelete = async () => {
    if (!deleteTarget) return;
    setActionLoading(deleteTarget.id);
    try {
      await onDelete(deleteTarget.id);
      setDeleteTarget(null);
    } catch {
      // Error handled by parent
    } finally {
      setActionLoading(null);
    }
  };

  function getJobStatusBadge(job: Job) {
    if (!job.enabled) {
      return <Badge variant="disabled">Disabled</Badge>;
    }
    if (job.last_exit_code === null && !job.last_run_at) {
      return <Badge variant="default">Pending</Badge>;
    }
    if (job.last_exit_code === 0) {
      return <Badge variant="success">OK</Badge>;
    }
    if (job.last_exit_code !== null) {
      return <Badge variant="error">Failed</Badge>;
    }
    return <Badge variant="default">Unknown</Badge>;
  }

  return (
    <>
      <div
        style={{
          overflowX: "auto",
          border: "1px solid var(--neutral-border-medium)",
          borderRadius: "var(--radius-l)",
        }}
      >
        <table className="data-table">
          <thead>
            <tr style={{ background: "var(--neutral-alpha-weak)" }}>
              <th>Status</th>
              <th
                style={{ cursor: "pointer", userSelect: "none" }}
                onClick={() => toggleSort("name")}
              >
                Name <SortIndicator field="name" />
              </th>
              <th>Schedule</th>
              <th>Type</th>
              <th
                style={{ cursor: "pointer", userSelect: "none" }}
                onClick={() => toggleSort("last_run_at")}
              >
                Last Run <SortIndicator field="last_run_at" />
              </th>
              <th>Next Run</th>
              <th style={{ textAlign: "right" }}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {sorted.map((job) => (
              <tr key={job.id}>
                <td>{getJobStatusBadge(job)}</td>
                <td>
                  <Link
                    href={`/jobs/${job.id}`}
                    style={{
                      fontWeight: 500,
                      color: "var(--neutral-on-background-strong)",
                      textDecoration: "none",
                    }}
                  >
                    {job.name}
                  </Link>
                </td>
                <td style={{ fontFamily: "var(--font-code)", fontSize: "var(--font-size-body-xs)" }}>
                  {job.schedule}
                </td>
                <td>
                  {job.execution.type === "ShellCommand" ? "Shell" : "Script"}
                </td>
                <td>{formatDate(job.last_run_at)}</td>
                <td>{job.enabled ? formatDate(job.next_run_at) : "--"}</td>
                <td>
                  <Row gap="4" horizontal="end" vertical="center">
                    <IconButton
                      icon="play"
                      variant="tertiary"
                      size="s"
                      onClick={() =>
                        handleAction(job.id, () => onTrigger(job.id))
                      }
                      disabled={actionLoading === job.id}
                      tooltip="Trigger now"
                    />
                    <Toggle
                      checked={job.enabled}
                      onChange={() =>
                        handleAction(
                          job.id,
                          job.enabled
                            ? () => onDisable(job.id)
                            : () => onEnable(job.id)
                        )
                      }
                      disabled={actionLoading === job.id}
                    />
                    <Link href={`/jobs/${job.id}/edit`}>
                      <IconButton
                        icon="edit"
                        variant="tertiary"
                        size="s"
                        tooltip="Edit"
                      />
                    </Link>
                    <IconButton
                      icon="delete"
                      variant="tertiary"
                      size="s"
                      onClick={() => setDeleteTarget(job)}
                      disabled={actionLoading === job.id}
                      tooltip="Delete"
                    />
                  </Row>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* Delete Confirmation Modal */}
      <Modal
        open={!!deleteTarget}
        onClose={() => setDeleteTarget(null)}
        title="Delete Job"
        actions={
          <>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setDeleteTarget(null)}
            >
              Cancel
            </Button>
            <Button
              variant="danger"
              size="sm"
              onClick={handleDelete}
              disabled={actionLoading !== null}
            >
              {actionLoading ? "Deleting..." : "Delete"}
            </Button>
          </>
        }
      >
        <Text variant="body-default-s" onBackground="neutral-medium">
          Are you sure you want to delete{" "}
          <strong>{deleteTarget?.name}</strong>? This action cannot be undone.
        </Text>
      </Modal>
    </>
  );
}
