"use client";

import React from "react";
import {
  Column,
  Row,
  Text,
  Heading,
  Input,
  Textarea,
  Select,
  Switch,
  Button,
  SegmentedControl,
  Line,
} from "@once-ui-system/core";
import { CronInput } from "./CronInput";
import { EnvVarsEditor } from "./EnvVarsEditor";
import { COMMON_TIMEZONES } from "@/lib/cron";

/* ── Job Details fields (no card wrapper) ── */
export function JobDetailsFields({
  name, setName, execType, setExecType, execValue, setExecValue, errors,
}: {
  name: string; setName: (v: string) => void;
  execType: "ShellCommand" | "ScriptFile"; setExecType: (v: "ShellCommand" | "ScriptFile") => void;
  execValue: string; setExecValue: (v: string) => void;
  errors: Record<string, string>;
}) {
  return (
    <>
      <Input
        id="job-name"
        label="Job Name"
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="my-backup-job"
        error={!!errors.name}
        errorMessage={errors.name}
      />
      <Column gap="8">
        <Text variant="label-default-s" onBackground="neutral-weak">
          Execution Type
        </Text>
        <SegmentedControl
          fillWidth
          selected={execType}
          buttons={[
            { value: "ShellCommand", label: "Shell Command" },
            { value: "ScriptFile", label: "Script File" },
          ]}
          onToggle={(value) => setExecType(value as "ShellCommand" | "ScriptFile")}
        />
      </Column>
      {execType === "ShellCommand" ? (
        <Textarea
          id="command"
          label="Command"
          value={execValue}
          onChange={(e) => setExecValue(e.target.value)}
          placeholder="echo 'Hello World'"
          lines={4}
          error={!!errors.execValue}
          errorMessage={errors.execValue}
        />
      ) : (
        <Input
          id="script-path"
          label="Script Path"
          value={execValue}
          onChange={(e) => setExecValue(e.target.value)}
          placeholder="/path/to/script.sh"
          error={!!errors.execValue}
          errorMessage={errors.execValue}
        />
      )}
    </>
  );
}

/* ── Schedule fields (no card wrapper) ── */
export function ScheduleFields({
  schedule, setSchedule, timezone, setTimezone, errors,
}: {
  schedule: string; setSchedule: (v: string) => void;
  timezone: string; setTimezone: (v: string) => void;
  errors: Record<string, string>;
}) {
  return (
    <>
      <Select
        id="timezone"
        label="Timezone"
        value={timezone}
        options={[
          { label: "System default", value: "" },
          ...COMMON_TIMEZONES.map((tz) => ({ label: tz, value: tz })),
        ]}
        onSelect={(value) => setTimezone(value)}
        searchable
      />
      <CronInput value={schedule} onChange={setSchedule} error={errors.schedule} />
    </>
  );
}

/* ── Runtime / Advanced fields (no card wrapper) ── */
export function RuntimeFields({
  workingDir, setWorkingDir, timeoutSecs, setTimeoutSecs, errors,
}: {
  workingDir: string; setWorkingDir: (v: string) => void;
  timeoutSecs: number; setTimeoutSecs: (v: number) => void;
  errors: Record<string, string>;
}) {
  return (
    <>
      <Input
        id="working-dir"
        label="Working Directory"
        value={workingDir}
        onChange={(e) => setWorkingDir(e.target.value)}
        placeholder="/home/user/project"
        description="Leave empty to use the daemon's working directory"
      />
      <Input
        id="timeout"
        label="Timeout (seconds)"
        type="number"
        min={1}
        value={timeoutSecs}
        onChange={(e) => setTimeoutSecs(parseInt(e.target.value, 10) || 3600)}
        error={!!errors.timeout}
        errorMessage={errors.timeout}
      />
    </>
  );
}

/* ── Environment Variables fields (no card wrapper) ── */
export function EnvVarsFields({
  envVars, setEnvVars, logEnvironment, setLogEnvironment,
}: {
  envVars: Record<string, string>;
  setEnvVars: (v: Record<string, string>) => void;
  logEnvironment: boolean; setLogEnvironment: (v: boolean) => void;
}) {
  return (
    <>
      <EnvVarsEditor value={envVars} onChange={setEnvVars} />
      <Line />
      <Column gap="8">
        <Switch
          id="log-environment"
          label="Log environment variables on run"
          isChecked={logEnvironment}
          onToggle={() => setLogEnvironment(!logEnvironment)}
        />
        <Row
          paddingX="12" paddingY="8" radius="s" gap="8"
          vertical="center"
          style={{
            background: "var(--warning-alpha-weak)",
            border: "1px solid var(--warning-border-medium)",
          }}
        >
          <Text variant="label-default-xs" style={{ color: "var(--warning-on-background-strong)" }}>
            Warning:
          </Text>
          <Text variant="body-default-xs" style={{ color: "var(--warning-on-background-medium)" }}>
            Not safe for production. Use only for temporary debugging.
          </Text>
        </Row>
      </Column>
    </>
  );
}

/* ── Reusable card wrapper ── */
export function SectionCard({
  title, subtitle, children,
}: {
  title: string; subtitle: string; children: React.ReactNode;
}) {
  return (
    <Column
      fillWidth padding="24" radius="l"
      border="neutral-alpha-medium" background="surface" gap="20"
    >
      <Column gap="4">
        <Heading variant="heading-strong-s">{title}</Heading>
        <Text variant="body-default-xs" onBackground="neutral-weak">{subtitle}</Text>
      </Column>
      <Line />
      {children}
    </Column>
  );
}

/* ── Top bar (shared) ── */
export function FormTopBar({
  title, enabled, setEnabled, submitLabel, submitting,
}: {
  title: string; enabled: boolean; setEnabled: (v: boolean) => void;
  submitLabel: string; submitting: boolean;
}) {
  return (
    <>
      <Row fillWidth horizontal="between" vertical="center" gap="16">
        <Heading variant="heading-strong-l">{title}</Heading>
        <Row gap="12" vertical="center">
          <Switch
            id="enabled"
            label={enabled ? "Enabled" : "Disabled"}
            isChecked={enabled}
            onToggle={() => setEnabled(!enabled)}
          />
          <Button type="submit" variant="primary" size="s" disabled={submitting}>
            {submitting ? "Saving..." : submitLabel}
          </Button>
          <Button
            type="button" variant="secondary" size="s"
            onClick={() => window.history.back()}
          >
            Cancel
          </Button>
        </Row>
      </Row>
      <Line />
    </>
  );
}
