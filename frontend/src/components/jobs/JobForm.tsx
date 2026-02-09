"use client";

import React, { useState } from "react";
import { Column, Row, SegmentedControl } from "@once-ui-system/core";
import {
  FormTopBar, SectionCard,
  JobDetailsFields, ScheduleFields, RuntimeFields, EnvVarsFields,
} from "./FormSections";
import { useJobFormState } from "./useJobFormState";
import type { Job, NewJob } from "@/lib/types";

interface JobFormProps {
  job?: Job | null;
  title: string;
  onSubmit: (data: NewJob) => Promise<void>;
  submitLabel?: string;
}

export function JobForm({ job, title, onSubmit, submitLabel = "Save" }: JobFormProps) {
  const s = useJobFormState(job);
  const [tab, setTab] = useState<"basic" | "advanced">("basic");

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!s.validate()) return;
    s.setSubmitting(true);
    try {
      await onSubmit(s.buildData());
    } catch {
      // Error handled by caller
    } finally {
      s.setSubmitting(false);
    }
  };

  return (
    <form onSubmit={handleSubmit}>
      <Column gap="24" fillWidth>
        <FormTopBar
          title={title}
          enabled={s.enabled}
          setEnabled={s.setEnabled}
          submitLabel={submitLabel}
          submitting={s.submitting}
        />

        <SegmentedControl
          fillWidth={false}
          selected={tab}
          buttons={[
            { value: "basic", label: "Basic" },
            { value: "advanced", label: "Advanced" },
          ]}
          onToggle={(v) => setTab(v as "basic" | "advanced")}
        />

        {tab === "basic" ? (
          <Row gap="20" fillWidth style={{ alignItems: "flex-start" }}>
            <SectionCard title="Job Details" subtitle="Name and command to execute">
              <JobDetailsFields
                name={s.name} setName={s.setName}
                execType={s.execType} setExecType={s.setExecType}
                execValue={s.execValue} setExecValue={s.setExecValue}
                errors={s.errors}
              />
            </SectionCard>
            <SectionCard title="Schedule" subtitle="When and how often the job runs">
              <ScheduleFields
                schedule={s.schedule} setSchedule={s.setSchedule}
                timezone={s.timezone} setTimezone={s.setTimezone}
                errors={s.errors}
              />
            </SectionCard>
          </Row>
        ) : (
          <Row gap="20" fillWidth style={{ alignItems: "flex-start" }}>
            <SectionCard title="Runtime" subtitle="Execution configuration">
              <RuntimeFields
                workingDir={s.workingDir} setWorkingDir={s.setWorkingDir}
                timeoutSecs={s.timeoutSecs} setTimeoutSecs={s.setTimeoutSecs}
                errors={s.errors}
              />
            </SectionCard>
            <SectionCard title="Environment" subtitle="Variables passed to the job process">
              <EnvVarsFields
                envVars={s.envVars} setEnvVars={s.setEnvVars}
                logEnvironment={s.logEnvironment} setLogEnvironment={s.setLogEnvironment}
              />
            </SectionCard>
          </Row>
        )}
      </Column>
    </form>
  );
}
