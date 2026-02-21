"use client";

import React, { useState } from "react";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
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
  const [tab, setTab] = useState<string>("basic");

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!s.validate()) {
      // If any validation errors are on advanced-tab fields, auto-switch
      // to that tab so the user can see them. We check the conditions
      // directly since state updates from validate() haven't flushed yet.
      const hasAdvancedError = s.timeoutSecs < 1;
      if (hasAdvancedError && tab !== "advanced") {
        setTab("advanced");
      }
      return;
    }
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
      <div className="flex flex-col gap-6 w-full">
        <FormTopBar
          title={title}
          enabled={s.enabled}
          setEnabled={s.setEnabled}
          submitLabel={submitLabel}
          submitting={s.submitting}
        />

        <Tabs value={tab} onValueChange={setTab}>
          <TabsList>
            <TabsTrigger value="basic">Basic</TabsTrigger>
            <TabsTrigger value="advanced">Advanced</TabsTrigger>
          </TabsList>

          <TabsContent value="basic">
            <div className="flex flex-col md:flex-row gap-5 items-start w-full">
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
            </div>
          </TabsContent>

          <TabsContent value="advanced">
            <div className="flex flex-col md:flex-row gap-5 items-start w-full">
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
            </div>
          </TabsContent>
        </Tabs>
      </div>
    </form>
  );
}
