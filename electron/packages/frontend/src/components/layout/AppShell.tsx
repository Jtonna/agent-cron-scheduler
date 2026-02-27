"use client";

import { BrowserRouter, Routes, Route } from "react-router-dom";
import { SSEProvider } from "@/hooks/useSSE";
import { Toaster } from "sonner";
import { ConnectionBanner } from "@/components/ConnectionBanner";
import { Sidebar } from "./Sidebar";
import { SidebarProvider, SidebarInset, SidebarTrigger } from "@/components/ui/sidebar";
import { DashboardPage } from "@/routes/DashboardPage";
import { AllJobsPage } from "@/routes/AllJobsPage";
import { CreateJobPage } from "@/routes/CreateJobPage";
import { JobDetailPage } from "@/routes/JobDetailPage";
import { EditJobPage } from "@/routes/EditJobPage";
import { RunLogPage } from "@/routes/RunLogPage";
import { SystemLogsPage } from "@/routes/SystemLogsPage";

export function AppShell() {
  return (
    <BrowserRouter>
      <SSEProvider>
        <SidebarProvider>
          <Sidebar />
          <SidebarInset className="overflow-y-auto">
            <ConnectionBanner />
            <header className="flex items-center gap-2 p-4 md:hidden">
              <SidebarTrigger />
              <span className="text-lg font-semibold">ACS</span>
            </header>
            <div className="p-6">
              <Routes>
                <Route path="/" element={<DashboardPage />} />
                <Route path="/jobs" element={<AllJobsPage />} />
                <Route path="/jobs/create" element={<CreateJobPage />} />
                <Route path="/jobs/:id" element={<JobDetailPage />} />
                <Route path="/jobs/:id/edit" element={<EditJobPage />} />
                <Route path="/jobs/:id/runs/:runId" element={<RunLogPage />} />
                <Route path="/logs" element={<SystemLogsPage />} />
              </Routes>
            </div>
          </SidebarInset>
        </SidebarProvider>
        <Toaster richColors />
      </SSEProvider>
    </BrowserRouter>
  );
}
