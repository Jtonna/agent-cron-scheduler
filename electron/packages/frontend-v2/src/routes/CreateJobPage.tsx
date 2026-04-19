"use client";

import { Link, useNavigate } from "react-router-dom";
import { ArrowLeftIcon } from "@heroicons/react/24/outline";
import { JobForm } from "@/components/jobs/JobForm";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { toast } from "sonner";
import type { NewJob } from "@/lib/types";

export function CreateJobPage() {
  const navigate = useNavigate();

  const handleSubmit = async (data: NewJob) => {
    try {
      const job = await api.createJob(data);
      toast.success("Job created successfully");
      navigate(`/jobs/${job.id}`);
    } catch (err) {
      toast.error(
        `Failed to create job: ${err instanceof Error ? err.message : "Unknown error"}`
      );
      throw err;
    }
  };

  return (
    <div className="w-full flex flex-col gap-6">
      <Button variant="ghost" size="sm" asChild>
        <Link to="/jobs">
          <ArrowLeftIcon className="h-4 w-4 mr-1.5" />
          Back to Jobs
        </Link>
      </Button>
      <JobForm
        title="Create New Job"
        onSubmit={handleSubmit}
        submitLabel="Create Job"
      />
    </div>
  );
}
