"use client";

import React, { useCallback } from "react";
import Link from "next/link";
import { Column, Row, Text } from "@once-ui-system/core";
import { useJobs } from "@/hooks/useJobs";
import { JobTable } from "@/components/jobs/JobTable";
import { Button } from "@/components/ui/Button";
import { Spinner } from "@/components/ui/Spinner";
import { useToast } from "@/components/ui/Toast";
import { useSSEEvents } from "@/hooks/useSSE";

export default function AllJobsPage() {
  const {
    jobs,
    loading,
    error,
    refresh,
    triggerJob,
    enableJob,
    disableJob,
    deleteJob,
  } = useJobs();
  const { addToast } = useToast();

  useSSEEvents(
    useCallback(
      (event) => {
        if (
          event.type === "job_changed" ||
          event.type === "completed" ||
          event.type === "failed"
        ) {
          refresh();
        }
      },
      [refresh]
    )
  );

  const handleTrigger = async (id: string) => {
    try {
      await triggerJob(id);
      addToast("Job triggered successfully", "success");
    } catch (err) {
      addToast(
        `Failed to trigger job: ${err instanceof Error ? err.message : "Unknown error"}`,
        "error"
      );
    }
  };

  const handleEnable = async (id: string) => {
    try {
      await enableJob(id);
      addToast("Job enabled", "success");
    } catch (err) {
      addToast(
        `Failed to enable job: ${err instanceof Error ? err.message : "Unknown error"}`,
        "error"
      );
    }
  };

  const handleDisable = async (id: string) => {
    try {
      await disableJob(id);
      addToast("Job disabled", "info");
    } catch (err) {
      addToast(
        `Failed to disable job: ${err instanceof Error ? err.message : "Unknown error"}`,
        "error"
      );
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await deleteJob(id);
      addToast("Job deleted", "success");
    } catch (err) {
      addToast(
        `Failed to delete job: ${err instanceof Error ? err.message : "Unknown error"}`,
        "error"
      );
    }
  };

  return (
    <Column gap="24" fillWidth>
      <Row horizontal="between" vertical="center" fillWidth>
        <Text variant="heading-strong-l">All Jobs</Text>
        <Link href="/jobs/new">
          <Button variant="primary">Create Job</Button>
        </Link>
      </Row>

      {loading && jobs.length === 0 && (
        <Row horizontal="center" paddingY="48">
          <Spinner size="lg" />
        </Row>
      )}

      {error && (
        <Column
          padding="16"
          radius="l"
          style={{
            background: "var(--danger-alpha-weak)",
            border: "1px solid var(--danger-border-medium)",
          }}
        >
          <Text variant="body-default-s" onBackground="danger-strong">
            {error}
          </Text>
        </Column>
      )}

      {!loading && (
        <JobTable
          jobs={jobs}
          onTrigger={handleTrigger}
          onEnable={handleEnable}
          onDisable={handleDisable}
          onDelete={handleDelete}
        />
      )}
    </Column>
  );
}
