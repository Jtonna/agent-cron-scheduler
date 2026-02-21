"use client";

import { SSEProvider } from "@/hooks/useSSE";
import { Toaster } from "sonner";
import { ConnectionBanner } from "@/components/ConnectionBanner";
import { Sidebar } from "./Sidebar";
import {
  SidebarProvider,
  SidebarInset,
  SidebarTrigger,
} from "@/components/ui/sidebar";

export function AppShell({ children }: { children: React.ReactNode }) {
  return (
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
            {children}
          </div>
        </SidebarInset>
      </SidebarProvider>
      <Toaster richColors />
    </SSEProvider>
  );
}
