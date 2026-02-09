"use client";

import React from "react";
import { useParams, useRouter } from "next/navigation";
import { Column, Row, Text } from "@once-ui-system/core";
import { useJob } from "@/hooks/useJob";
import { JobForm } from "@/components/jobs/JobForm";
import { Spinner } from "@/components/ui/Spinner";
import { api } from "@/lib/api";
import { useToast } from "@/components/ui/Toast";
import type { NewJob } from "@/lib/types";

export function EditJobContent() {
  const params = useParams();
  const id = params.id as string;
  const router = useRouter();
  const { job, loading, error } = useJob(id);
  const { addToast } = useToast();

  const handleSubmit = async (data: NewJob) => {
    try {
      await api.updateJob(id, data);
      addToast("Job updated successfully", "success");
      router.push(`/jobs/${id}`);
    } catch (err) {
      addToast(
        `Failed to update job: ${err instanceof Error ? err.message : "Unknown error"}`,
        "error"
      );
      throw err;
    }
  };

  if (loading) {
    return (
      <Row horizontal="center" paddingY="48">
        <Spinner size="lg" />
      </Row>
    );
  }

  if (error || !job) {
    return (
      <Column
        padding="16"
        radius="l"
        style={{
          background: "var(--danger-alpha-weak)",
          border: "1px solid var(--danger-border-medium)",
        }}
      >
        <Text variant="body-default-s" onBackground="danger-strong">
          {error || "Job not found"}
        </Text>
      </Column>
    );
  }

  return (
    <Column fillWidth>
      <JobForm
        job={job}
        title={`Edit Job: ${job.name}`}
        onSubmit={handleSubmit}
        submitLabel="Update Job"
      />
    </Column>
  );
}
