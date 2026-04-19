"use client";

import { useNavigate, useParams } from "react-router-dom";
import { ArrowPathIcon } from "@heroicons/react/24/outline";
import { useJob } from "@/hooks/useJob";
import { JobForm } from "@/components/jobs/JobForm";
import { api } from "@/lib/api";
import { toast } from "sonner";
import type { NewJob } from "@/lib/types";

export function EditJobPage() {
  const navigate = useNavigate();
  const { id } = useParams<{ id: string }>();
  const { job, loading, error } = useJob(id!);

  const handleSubmit = async (data: NewJob) => {
    try {
      await api.updateJob(id!, data);
      toast.success("Job updated successfully");
      navigate(`/jobs/${id}`);
    } catch (err) {
      toast.error(
        `Failed to update job: ${err instanceof Error ? err.message : "Unknown error"}`
      );
      throw err;
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12">
        <ArrowPathIcon className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (error || !job) {
    return (
      <div className="rounded-lg border border-destructive/50 bg-destructive/10 p-4">
        <p className="text-sm text-destructive">
          {error || "Job not found"}
        </p>
      </div>
    );
  }

  return (
    <div className="w-full">
      <JobForm
        job={job}
        title={`Edit Job: ${job.name}`}
        onSubmit={handleSubmit}
        submitLabel="Update Job"
      />
    </div>
  );
}
