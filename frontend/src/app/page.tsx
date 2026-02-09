"use client";

import React, { useCallback } from "react";
import { Column, Row, Text } from "@once-ui-system/core";
import { useHealth } from "@/hooks/useHealth";
import { DashboardGrid } from "@/components/dashboard/DashboardGrid";
import { Spinner } from "@/components/ui/Spinner";
import { useSSEEvents } from "@/hooks/useSSE";

export default function DashboardPage() {
  const { health, loading, error, refresh } = useHealth(5000);

  useSSEEvents(
    useCallback(
      (event) => {
        if (event.type === "job_changed") {
          refresh();
        }
      },
      [refresh]
    )
  );

  return (
    <Column gap="24" fillWidth>
      <Text variant="heading-strong-l">Dashboard</Text>

      {loading && !health && (
        <Row horizontal="center" paddingY="48">
          <Spinner size="lg" />
        </Row>
      )}

      {error && !health && (
        <Column
          padding="16"
          radius="l"
          style={{
            background: "var(--danger-alpha-weak)",
            border: "1px solid var(--danger-border-medium)",
          }}
        >
          <Text variant="body-default-s" onBackground="danger-strong">
            Failed to connect to server: {error}
          </Text>
        </Column>
      )}

      {health && <DashboardGrid health={health} />}
    </Column>
  );
}
