"use client";

import React from "react";
import { useNavigate } from "react-router-dom";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Separator } from "@/components/ui/separator";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { CronInput } from "./CronInput";
import { EnvVarsEditor } from "./EnvVarsEditor";
import { COMMON_TIMEZONES } from "@/lib/cron";
import { cn } from "@/lib/utils";

/* -- Job Details fields (no card wrapper) -- */
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
      <div className="flex flex-col gap-2">
        <Label htmlFor="job-name">Job Name</Label>
        <Input
          id="job-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="my-backup-job"
          className={cn(errors.name && "border-destructive")}
        />
        {errors.name && (
          <p className="text-xs text-destructive">{errors.name}</p>
        )}
      </div>
      <div className="flex flex-col gap-2">
        <Label>Execution Type</Label>
        <Tabs value={execType} onValueChange={(v) => setExecType(v as "ShellCommand" | "ScriptFile")}>
          <TabsList className="w-full">
            <TabsTrigger value="ShellCommand" className="flex-1">Shell Command</TabsTrigger>
            <TabsTrigger value="ScriptFile" className="flex-1">Script File</TabsTrigger>
          </TabsList>
        </Tabs>
      </div>
      {execType === "ShellCommand" ? (
        <div className="flex flex-col gap-2">
          <Label htmlFor="command">Command</Label>
          <Textarea
            id="command"
            value={execValue}
            onChange={(e) => setExecValue(e.target.value)}
            placeholder="echo 'Hello World'"
            rows={4}
            className={cn("font-mono", errors.execValue && "border-destructive")}
          />
          {errors.execValue && (
            <p className="text-xs text-destructive">{errors.execValue}</p>
          )}
        </div>
      ) : (
        <div className="flex flex-col gap-2">
          <Label htmlFor="script-path">Script Path</Label>
          <Input
            id="script-path"
            value={execValue}
            onChange={(e) => setExecValue(e.target.value)}
            placeholder="/path/to/script.sh"
            className={cn(errors.execValue && "border-destructive")}
          />
          {errors.execValue && (
            <p className="text-xs text-destructive">{errors.execValue}</p>
          )}
        </div>
      )}
    </>
  );
}

/* -- Schedule fields (no card wrapper) -- */
export function ScheduleFields({
  schedule, setSchedule, timezone, setTimezone, errors,
}: {
  schedule: string; setSchedule: (v: string) => void;
  timezone: string; setTimezone: (v: string) => void;
  errors: Record<string, string>;
}) {
  return (
    <>
      <div className="flex flex-col gap-2">
        <Label>Timezone</Label>
        <Select
          value={timezone || "__system_default__"}
          onValueChange={(v) => setTimezone(v === "__system_default__" ? "" : v)}
        >
          <SelectTrigger>
            <SelectValue placeholder="System default" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="__system_default__">System default</SelectItem>
            {COMMON_TIMEZONES.map((tz) => (
              <SelectItem key={tz} value={tz}>
                {tz}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <CronInput value={schedule} onChange={setSchedule} error={errors.schedule} />
    </>
  );
}

/* -- Runtime / Advanced fields (no card wrapper) -- */
export function RuntimeFields({
  workingDir, setWorkingDir, timeoutSecs, setTimeoutSecs,
  allowConcurrent, setAllowConcurrent, errors,
}: {
  workingDir: string; setWorkingDir: (v: string) => void;
  timeoutSecs: number; setTimeoutSecs: (v: number) => void;
  allowConcurrent: boolean; setAllowConcurrent: (v: boolean) => void;
  errors: Record<string, string>;
}) {
  return (
    <>
      <div className="flex flex-col gap-2">
        <Label htmlFor="working-dir">Working Directory</Label>
        <Input
          id="working-dir"
          value={workingDir}
          onChange={(e) => setWorkingDir(e.target.value)}
          placeholder="/home/user/project"
        />
        <p className="text-xs text-muted-foreground">
          Leave empty to use the daemon&apos;s working directory
        </p>
      </div>
      <div className="flex flex-col gap-2">
        <Label htmlFor="timeout">Timeout (seconds)</Label>
        <Input
          id="timeout"
          type="number"
          min={0}
          value={timeoutSecs}
          onChange={(e) => { const parsed = parseInt(e.target.value, 10); setTimeoutSecs(Number.isNaN(parsed) ? 0 : parsed); }}
          className={cn(errors.timeout && "border-destructive")}
        />
        <p className="text-xs text-muted-foreground">
          Set to 0 for no timeout limit
        </p>
        {errors.timeout && (
          <p className="text-xs text-destructive">{errors.timeout}</p>
        )}
      </div>
      <div className="flex flex-col gap-3">
        <div className="flex items-center gap-3">
          <Switch
            id="allow-concurrent"
            checked={allowConcurrent}
            onCheckedChange={(checked) => setAllowConcurrent(checked)}
          />
          <Label htmlFor="allow-concurrent">Allow Concurrent Runs</Label>
        </div>
        <p className="text-xs text-muted-foreground">
          Allow multiple instances of this job to run simultaneously
        </p>
      </div>
    </>
  );
}

/* -- Environment Variables fields (no card wrapper) -- */
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
      <Separator />
      <div className="flex flex-col gap-3">
        <div className="flex items-center gap-3">
          <Switch
            id="log-environment"
            checked={logEnvironment}
            onCheckedChange={(checked) => setLogEnvironment(checked)}
          />
          <Label htmlFor="log-environment">Log environment variables on run</Label>
        </div>
        <div className="flex items-center gap-2 rounded-md bg-accent/50 border border-accent px-3 py-2">
          <span className="text-xs font-semibold text-accent-foreground">Warning:</span>
          <span className="text-xs text-muted-foreground">
            Not safe for production. Use only for temporary debugging.
          </span>
        </div>
      </div>
    </>
  );
}

/* -- Reusable card wrapper -- */
export function SectionCard({
  title, subtitle, children,
}: {
  title: string; subtitle: string; children: React.ReactNode;
}) {
  return (
    <Card className="flex-1 w-full">
      <CardHeader>
        <CardTitle className="text-base">{title}</CardTitle>
        <CardDescription>{subtitle}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        {children}
      </CardContent>
    </Card>
  );
}

/* -- Top bar (shared) -- */
export function FormTopBar({
  title, enabled, setEnabled, submitLabel, submitting,
}: {
  title: string; enabled: boolean; setEnabled: (v: boolean) => void;
  submitLabel: string; submitting: boolean;
}) {
  const navigate = useNavigate();

  return (
    <>
      <div className="flex items-center justify-between gap-4 w-full">
        <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-2">
            <Switch
              id="enabled"
              checked={enabled}
              onCheckedChange={(checked) => setEnabled(checked)}
            />
            <Label htmlFor="enabled">{enabled ? "Enabled" : "Disabled"}</Label>
          </div>
          <Button type="submit" size="sm" disabled={submitting}>
            {submitting ? "Saving..." : submitLabel}
          </Button>
          <Button
            type="button"
            variant="secondary"
            size="sm"
            onClick={() => navigate(-1)}
          >
            Cancel
          </Button>
        </div>
      </div>
      <Separator />
    </>
  );
}
