"use client";

import { BrowserRouter, Routes, Route, useLocation } from "react-router-dom";
import { Toaster } from "sonner";
import { SSEProvider } from "@/hooks/useSSE";
import { ConnectionBanner } from "@/components/ConnectionBanner";
import { TopBar } from "@/components/layout/TopBar";
import { JobsSidebar } from "@/components/layout/JobsSidebar";
import { MonitoringPage } from "@/routes/MonitoringPage";
import { AllJobsPage } from "@/routes/AllJobsPage";
import { CreateJobPage } from "@/routes/CreateJobPage";
import { JobDetailPage } from "@/routes/JobDetailPage";
import { EditJobPage } from "@/routes/EditJobPage";
import { RunLogPage } from "@/routes/RunLogPage";

function AppLayout() {
  const location = useLocation();
  const isJobsRoute = location.pathname.startsWith("/jobs");

  return (
    <>
      <ConnectionBanner />
      <TopBar />
      <div className="flex pt-[49px]">
        {isJobsRoute && <JobsSidebar />}
        <main
          className={
            isJobsRoute
              ? "ml-[256px] flex-1 overflow-y-auto"
              : "flex-1 overflow-y-auto"
          }
          style={{ minHeight: "calc(100vh - 49px)" }}
        >
          <Routes>
            <Route path="/" element={<MonitoringPage />} />
            <Route path="/jobs" element={<AllJobsPage />} />
            <Route path="/jobs/create" element={<CreateJobPage />} />
            <Route path="/jobs/:id" element={<JobDetailPage />} />
            <Route path="/jobs/:id/edit" element={<EditJobPage />} />
            <Route path="/jobs/:id/runs/:runId" element={<RunLogPage />} />
          </Routes>
        </main>
      </div>
      <Toaster richColors />
    </>
  );
}

export function AppShell() {
  return (
    <BrowserRouter>
      <SSEProvider>
        <AppLayout />
      </SSEProvider>
    </BrowserRouter>
  );
}
