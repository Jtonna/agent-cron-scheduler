"use client";

import React, { useState, useEffect, useRef, useMemo } from "react";
import {
  Column,
  Row,
  Text,
  Input,
  Icon,
  IconButton,
  Switch,
  SegmentedControl,
  Spinner,
} from "@once-ui-system/core";
import { useSystemLogs } from "@/hooks/useSystemLogs";

export default function SystemLogsPage() {
  const [tail, setTail] = useState(200);
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [search, setSearch] = useState("");
  const { logs, loading, error, refresh } = useSystemLogs(tail);
  const preRef = useRef<HTMLPreElement>(null);

  // Auto-refresh polling
  useEffect(() => {
    if (!autoRefresh) return;
    const timer = setInterval(refresh, 3000);
    return () => clearInterval(timer);
  }, [autoRefresh, refresh]);

  // Auto-scroll on new content
  useEffect(() => {
    if (preRef.current) {
      preRef.current.scrollTop = preRef.current.scrollHeight;
    }
  }, [logs]);

  const filteredLogs = useMemo(() => {
    if (!logs) return "";
    if (!search.trim()) return logs;
    const query = search.toLowerCase();
    return logs
      .split("\n")
      .filter((line) => line.toLowerCase().includes(query))
      .join("\n");
  }, [logs, search]);

  return (
    <Column gap="20" fillWidth>
      <Text variant="heading-strong-l">System Logs</Text>

      {/* Controls */}
      <Row gap="12" vertical="center" fillWidth wrap>
        <Input
          id="log-search"
          height="s"
          placeholder="Filter logs..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          hasPrefix={
            <Icon marginLeft="4" name="search" size="xs" onBackground="neutral-weak" />
          }
          hasSuffix={
            search.length > 0 ? (
              <IconButton
                variant="ghost"
                icon="close"
                size="s"
                onClick={() => setSearch("")}
                aria-label="Clear search"
              />
            ) : null
          }
          style={{ flex: 1, minWidth: "200px" }}
        />
        <SegmentedControl
          fillWidth={false}
          defaultSelected="200"
          buttons={[
            { value: "100", label: "100" },
            { value: "200", label: "200" },
            { value: "500", label: "500" },
            { value: "1000", label: "1K" },
          ]}
          onToggle={(value) => setTail(Number(value))}
        />
        <Switch
          id="auto-refresh"
          label="Auto-refresh"
          isChecked={autoRefresh}
          onToggle={() => setAutoRefresh(!autoRefresh)}
        />
      </Row>

      {/* Error state */}
      {error && (
        <Column
          padding="16"
          radius="l"
          background="danger-weak"
          border="danger-medium"
        >
          <Text variant="body-default-s" onBackground="danger-strong">
            {error}
          </Text>
        </Column>
      )}

      {/* Log viewer */}
      {loading && !logs ? (
        <Row horizontal="center" paddingY="48">
          <Spinner size="l" />
        </Row>
      ) : (
        <pre
          ref={preRef}
          className="log-pre"
          style={{ maxHeight: "calc(100vh - 260px)", minHeight: "300px" }}
        >
          {filteredLogs || "No logs available."}
        </pre>
      )}
    </Column>
  );
}
