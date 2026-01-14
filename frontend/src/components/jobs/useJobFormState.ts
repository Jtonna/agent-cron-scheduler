"use client";

import { useState, useEffect } from "react";
import { validateCron } from "@/lib/cron";
import type { Job, NewJob } from "@/lib/types";

export function useJobFormState(job?: Job | null) {
  const [name, setName] = useState(job?.name ?? "");
  const [execType, setExecType] = useState<"ShellCommand" | "ScriptFile">(
    job?.execution.type ?? "ShellCommand"
  );
  const [execValue, setExecValue] = useState(job?.execution.value ?? "");
  const [schedule, setSchedule] = useState(job?.schedule ?? "*/5 * * * *");
  const [timezone, setTimezone] = useState(job?.timezone ?? "");
  const [workingDir, setWorkingDir] = useState(job?.working_dir ?? "");
  const [envVars, setEnvVars] = useState<Record<string, string>>(
    job?.env_vars ?? {}
  );
  const [timeoutSecs, setTimeoutSecs] = useState(job?.timeout_secs ?? 3600);
  const [logEnvironment, setLogEnvironment] = useState(
    job?.log_environment ?? false
  );
  const [enabled, setEnabled] = useState(job?.enabled ?? true);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [submitted, setSubmitted] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (job) {
      setName(job.name);
      setExecType(job.execution.type);
      setExecValue(job.execution.value);
      setSchedule(job.schedule);
      setTimezone(job.timezone ?? "");
      setWorkingDir(job.working_dir ?? "");
      setEnvVars(job.env_vars ?? {});
      setTimeoutSecs(job.timeout_secs);
      setLogEnvironment(job.log_environment);
      setEnabled(job.enabled);
    }
  }, [job]);

  const validate = (): boolean => {
    setSubmitted(true);
    const newErrors: Record<string, string> = {};
    if (!name.trim()) newErrors.name = "Name is required";
    if (!execValue.trim()) newErrors.execValue = "Command or script path is required";
    if (!schedule.trim()) newErrors.schedule = "Schedule is required";
    const cronErr = validateCron(schedule);
    if (cronErr) newErrors.schedule = cronErr;
    if (timeoutSecs < 1) newErrors.timeout = "Timeout must be at least 1 second";
    setErrors(newErrors);
    return Object.keys(newErrors).length === 0;
  };

  // Only expose errors after the user has attempted to submit
  const visibleErrors = submitted ? errors : {};

  const buildData = (): NewJob => {
    const data: NewJob = {
      name: name.trim(),
      schedule: schedule.trim(),
      execution: { type: execType, value: execValue.trim() },
      enabled,
      timeout_secs: timeoutSecs,
      log_environment: logEnvironment,
    };
    if (timezone) data.timezone = timezone;
    if (workingDir) data.working_dir = workingDir;
    if (Object.keys(envVars).length > 0) data.env_vars = envVars;
    return data;
  };

  const hasAdvancedFields = !!(
    job?.working_dir ||
    job?.log_environment ||
    (job?.env_vars && Object.keys(job.env_vars).length > 0) ||
    (job && job.timeout_secs !== 3600)
  );

  return {
    name, setName,
    execType, setExecType,
    execValue, setExecValue,
    schedule, setSchedule,
    timezone, setTimezone,
    workingDir, setWorkingDir,
    envVars, setEnvVars,
    timeoutSecs, setTimeoutSecs,
    logEnvironment, setLogEnvironment,
    enabled, setEnabled,
    errors: visibleErrors,
    submitting, setSubmitting,
    validate, buildData,
    hasAdvancedFields,
  };
}
