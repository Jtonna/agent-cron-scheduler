"use client";

import React from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { ArrowLeftIcon } from "@heroicons/react/24/outline";
import { JobForm } from "@/components/jobs/JobForm";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { toast } from "sonner";
import type { NewJob } from "@/lib/types";

export default function CreateJobPage() {
  const router = useRouter();

  const handleSubmit = async (data: NewJob) => {
    try {
      const job = await api.createJob(data);
      toast.success("Job created successfully");
      router.push(`/jobs/${job.id}`);
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
        <Link href="/jobs">
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
