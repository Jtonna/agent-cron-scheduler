"use client";

import React from "react";
import { useRouter } from "next/navigation";
import { Column } from "@once-ui-system/core";
import { JobForm } from "@/components/jobs/JobForm";
import { api } from "@/lib/api";
import { useToast } from "@/components/ui/Toast";
import type { NewJob } from "@/lib/types";

export default function CreateJobPage() {
  const router = useRouter();
  const { addToast } = useToast();

  const handleSubmit = async (data: NewJob) => {
    try {
      const job = await api.createJob(data);
      addToast("Job created successfully", "success");
      router.push(`/jobs/${job.id}`);
    } catch (err) {
      addToast(
        `Failed to create job: ${err instanceof Error ? err.message : "Unknown error"}`,
        "error"
      );
      throw err;
    }
  };

  return (
    <Column fillWidth>
      <JobForm
        title="Create New Job"
        onSubmit={handleSubmit}
        submitLabel="Create Job"
      />
    </Column>
  );
}
