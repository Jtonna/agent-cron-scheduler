"use client";

import React, { useState, useEffect, useCallback } from "react";
import { usePathname } from "next/navigation";
import {
  Column,
  Text,
  Line,
  ToggleButton,
} from "@once-ui-system/core";
import { SidebarSection } from "./SidebarSection";
import { RecentJobsWidget } from "./RecentJobsWidget";
import { Modal } from "@/components/ui/Modal";
import { Button } from "@/components/ui/Button";
import { api } from "@/lib/api";
import type { Job } from "@/lib/types";
import { useSSEEvents } from "@/hooks/useSSE";

export function Sidebar() {
  const pathname = usePathname();
  const [jobs, setJobs] = useState<Job[]>([]);
  const [showRestart, setShowRestart] = useState(false);
  const [showShutdown, setShowShutdown] = useState(false);
  const [actionLoading, setActionLoading] = useState(false);

  const navigate = (path: string) => {
    window.location.href = path;
  };

  const fetchJobs = useCallback(async () => {
    try {
      const data = await api.listJobs();
      setJobs(data);
    } catch {
      // Ignore errors silently in sidebar
    }
  }, []);

  useEffect(() => {
    fetchJobs();
    const timer = setInterval(fetchJobs, 10000);
    return () => clearInterval(timer);
  }, [fetchJobs]);

  useSSEEvents(
    useCallback(
      (event) => {
        if (
          event.type === "job_changed" ||
          event.type === "completed" ||
          event.type === "failed"
        ) {
          fetchJobs();
        }
      },
      [fetchJobs]
    )
  );

  const handleRestart = async () => {
    setActionLoading(true);
    try {
      await api.restart();
      setShowRestart(false);
    } catch {
      // Server might be restarting
    } finally {
      setActionLoading(false);
    }
  };

  const handleShutdown = async () => {
    setActionLoading(true);
    try {
      await api.shutdown();
      setShowShutdown(false);
    } catch {
      // Server is shutting down
    } finally {
      setActionLoading(false);
    }
  };

  return (
    <Column
      as="aside"
      background="surface"
      style={{
        width: "256px",
        minHeight: "100vh",
        flexShrink: 0,
        borderRight: "1px solid var(--neutral-border-medium)",
      }}
    >
      {/* Header */}
      <Column paddingX="20" paddingY="20">
        <Text variant="heading-strong-l">ACS</Text>
        <Text variant="body-default-xs" onBackground="neutral-weak" marginTop="4">
          Agent Cron Scheduler
        </Text>
      </Column>

      <Line />

      {/* Navigation */}
      <Column as="nav" fillWidth style={{ flex: 1, overflowY: "auto" }} padding="8" gap="8">
        <SidebarSection title="System">
          <ToggleButton
            fillWidth
            horizontal="start"
            prefixIcon="home"
            label="Dashboard"
            selected={pathname === "/"}
            onClick={() => navigate("/")}
          />
          <ToggleButton
            fillWidth
            horizontal="start"
            prefixIcon="fileText"
            label="Logs"
            selected={pathname === "/logs"}
            onClick={() => navigate("/logs")}
          />
        </SidebarSection>

        <SidebarSection title="Jobs">
          <ToggleButton
            fillWidth
            horizontal="start"
            prefixIcon="plusCircle"
            label="Create New Job"
            selected={pathname === "/jobs/new"}
            onClick={() => navigate("/jobs/new")}
          />
          <ToggleButton
            fillWidth
            horizontal="start"
            prefixIcon="listChecks"
            label="All Jobs"
            selected={pathname === "/jobs"}
            onClick={() => navigate("/jobs")}
          />
        </SidebarSection>

        <SidebarSection title="Recent Jobs">
          <RecentJobsWidget jobs={jobs} />
        </SidebarSection>
      </Column>

      {/* Footer actions */}
      <Line />
      <Column padding="8" gap="4">
        <ToggleButton
          fillWidth
          horizontal="start"
          prefixIcon="refresh"
          label="Restart"
          onClick={() => setShowRestart(true)}
        />
        <ToggleButton
          fillWidth
          horizontal="start"
          prefixIcon="power"
          label="Shutdown"
          onClick={() => setShowShutdown(true)}
        />
      </Column>

      {/* Restart Confirmation */}
      <Modal
        open={showRestart}
        onClose={() => setShowRestart(false)}
        title="Restart Server"
        actions={
          <>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setShowRestart(false)}
            >
              Cancel
            </Button>
            <Button
              variant="primary"
              size="sm"
              onClick={handleRestart}
              disabled={actionLoading}
            >
              {actionLoading ? "Restarting..." : "Restart"}
            </Button>
          </>
        }
      >
        <Text variant="body-default-s" onBackground="neutral-medium">
          Are you sure you want to restart the server? Active jobs will continue running.
        </Text>
      </Modal>

      {/* Shutdown Confirmation */}
      <Modal
        open={showShutdown}
        onClose={() => setShowShutdown(false)}
        title="Shutdown Server"
        actions={
          <>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setShowShutdown(false)}
            >
              Cancel
            </Button>
            <Button
              variant="danger"
              size="sm"
              onClick={handleShutdown}
              disabled={actionLoading}
            >
              {actionLoading ? "Shutting down..." : "Shutdown"}
            </Button>
          </>
        }
      >
        <Text variant="body-default-s" onBackground="neutral-medium">
          Are you sure you want to shut down the server? This will stop all scheduled jobs.
        </Text>
      </Modal>
    </Column>
  );
}
