"use client";

import React, { useState, useEffect, useCallback } from "react";

import { Link, useLocation } from "react-router-dom";
import {
  HomeIcon,
  DocumentTextIcon,
  PlusCircleIcon,
  ListBulletIcon,
  ArrowPathIcon,
  PowerIcon,
  SunIcon,
  MoonIcon,
  CommandLineIcon,
  ChevronRightIcon,
  ClockIcon,
  Cog6ToothIcon,
} from "@heroicons/react/24/outline";
import {
  CheckCircleIcon as CheckCircleSolid,
  XCircleIcon as XCircleSolid,
} from "@heroicons/react/20/solid";
import { useTheme } from "next-themes";
import {
  Sidebar as SidebarRoot,
  SidebarHeader,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarGroupContent,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuButton,
  SidebarMenuBadge,
  SidebarMenuSub,
  SidebarMenuSubItem,
  SidebarMenuSubButton,
} from "@/components/ui/sidebar";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { toast } from "sonner";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { api } from "@/lib/api";
import type { Job } from "@/lib/types";
import { useSSEEvents } from "@/hooks/useSSE";

interface NavItem {
  label: string;
  icon: React.ComponentType<React.SVGProps<SVGSVGElement>>;
  path: string;
}

interface NavSection {
  group: string;
  items: NavItem[];
}

export function Sidebar() {
  const { pathname } = useLocation();
  const { theme, setTheme } = useTheme();
  const [jobs, setJobs] = useState<Job[]>([]);
  const [showRestart, setShowRestart] = useState(false);
  const [showShutdown, setShowShutdown] = useState(false);
  const [actionLoading, setActionLoading] = useState(false);

  const fetchJobs = useCallback(async () => {
    try {
      const data = await api.listJobs();
      setJobs(data);
    } catch {
      // Ignore errors silently in sidebar
    }
  }, []);

  useEffect(() => {
    fetchJobs();
    const timer = setInterval(fetchJobs, 10000);
    return () => clearInterval(timer);
  }, [fetchJobs]);

  useSSEEvents(
    useCallback(
      (event) => {
        if (
          event.type === "job_changed" ||
          event.type === "completed" ||
          event.type === "failed"
        ) {
          fetchJobs();
        }
      },
      [fetchJobs]
    )
  );

  const handleRestart = async () => {
    setActionLoading(true);
    try {
      await api.restart();
      setShowRestart(false);
      toast.success("Server is restarting...");
    } catch {
      toast.error("Failed to restart server");
    } finally {
      setActionLoading(false);
    }
  };

  const handleShutdown = async () => {
    setActionLoading(true);
    try {
      await api.shutdown();
      setShowShutdown(false);
    } catch {
      toast.error("Failed to shut down server");
    } finally {
      setActionLoading(false);
    }
  };

  const navItems: NavSection[] = [
    {
      group: "System",
      items: [
        { label: "Dashboard", icon: HomeIcon, path: "/" },
        { label: "Logs", icon: DocumentTextIcon, path: "/logs" },
      ],
    },
    {
      group: "Jobs",
      items: [
        { label: "Create New Job", icon: PlusCircleIcon, path: "/jobs/create" },
        { label: "All Jobs", icon: ListBulletIcon, path: "/jobs" },
      ],
    },
  ];

  const recentJobs = jobs
    .filter((j) => j.last_run_at)
    .sort(
      (a, b) =>
        new Date(b.last_run_at!).getTime() -
        new Date(a.last_run_at!).getTime()
    )
    .slice(0, 7);

  return (
    <SidebarRoot collapsible="offcanvas">
      {/* Header - App branding */}
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton size="lg" className="pointer-events-none">
              <div className="flex aspect-square size-8 items-center justify-center rounded-lg bg-primary text-primary-foreground">
                <CommandLineIcon className="size-4" />
              </div>
              <div className="grid flex-1 text-left text-sm leading-tight">
                <span className="truncate font-semibold">ACS</span>
                <span className="truncate text-xs text-muted-foreground">
                  Agent Cron Scheduler
                </span>
              </div>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>

      {/* Navigation with Collapsible Groups */}
      <SidebarContent>
        {navItems.map((section) => (
          <Collapsible
            key={section.group}
            defaultOpen
            className="group/collapsible"
          >
            <SidebarGroup>
              <SidebarGroupLabel asChild>
                <CollapsibleTrigger>
                  {section.group}
                  <ChevronRightIcon className="ml-auto size-4 transition-transform duration-200 group-data-[state=open]/collapsible:rotate-90" />
                </CollapsibleTrigger>
              </SidebarGroupLabel>
              <CollapsibleContent>
                <SidebarGroupContent>
                  <SidebarMenu>
                    {section.items.map((item) => (
                      <SidebarMenuItem key={item.path}>
                        <SidebarMenuButton
                          asChild
                          isActive={pathname === item.path}
                          tooltip={item.label}
                        >
                          <Link to={item.path}>
                            <item.icon className="size-4" />
                            <span>{item.label}</span>
                          </Link>
                        </SidebarMenuButton>
                        {item.path === "/jobs" && jobs.length > 0 && (
                          <SidebarMenuBadge>{jobs.length}</SidebarMenuBadge>
                        )}
                      </SidebarMenuItem>
                    ))}
                  </SidebarMenu>
                </SidebarGroupContent>
              </CollapsibleContent>
            </SidebarGroup>
          </Collapsible>
        ))}

        {/* Recent Jobs as Collapsible Sub-Menu */}
        <SidebarGroup>
          <SidebarGroupLabel>Recent</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              <Collapsible defaultOpen className="group/collapsible">
                <SidebarMenuItem>
                  <CollapsibleTrigger asChild>
                    <SidebarMenuButton>
                      <ClockIcon className="size-4" />
                      <span>Recent Runs</span>
                      <ChevronRightIcon className="ml-auto size-4 transition-transform duration-200 group-data-[state=open]/collapsible:rotate-90" />
                    </SidebarMenuButton>
                  </CollapsibleTrigger>
                  <CollapsibleContent>
                    <SidebarMenuSub>
                      {recentJobs.length === 0 ? (
                        <SidebarMenuSubItem>
                          <SidebarMenuSubButton className="pointer-events-none text-sidebar-foreground/50">
                            <span className="text-xs">No recent runs</span>
                          </SidebarMenuSubButton>
                        </SidebarMenuSubItem>
                      ) : (
                        recentJobs.map((job) => (
                          <SidebarMenuSubItem key={job.id}>
                            <SidebarMenuSubButton
                              asChild
                              isActive={pathname === `/jobs/${job.id}`}
                            >
                              <Link to={`/jobs/${job.id}`}>
                                {job.last_exit_code === null ? (
                                  <ArrowPathIcon className="size-4 text-muted-foreground shrink-0" />
                                ) : job.last_exit_code === 0 ? (
                                  <CheckCircleSolid className="size-4 text-emerald-600 dark:text-emerald-400 shrink-0" />
                                ) : (
                                  <XCircleSolid className="size-4 text-red-600 dark:text-red-400 shrink-0" />
                                )}
                                <span>{job.name}</span>
                              </Link>
                            </SidebarMenuSubButton>
                          </SidebarMenuSubItem>
                        ))
                      )}
                    </SidebarMenuSub>
                  </CollapsibleContent>
                </SidebarMenuItem>
              </Collapsible>
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>

      {/* Footer */}
      <SidebarFooter>
        <SidebarMenu>
          <SidebarMenuItem>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <SidebarMenuButton tooltip="Settings">
                  <Cog6ToothIcon className="size-4" />
                  <span>Settings</span>
                </SidebarMenuButton>
              </DropdownMenuTrigger>
              <DropdownMenuContent
                side="top"
                align="start"
                className="w-[--radix-popper-anchor-width]"
              >
                <DropdownMenuItem
                  onClick={() =>
                    setTheme(theme === "dark" ? "light" : "dark")
                  }
                >
                  {theme === "dark" ? (
                    <SunIcon className="size-4" />
                  ) : (
                    <MoonIcon className="size-4" />
                  )}
                  <span>
                    {theme === "dark" ? "Light Mode" : "Dark Mode"}
                  </span>
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={() => setShowRestart(true)}>
                  <ArrowPathIcon className="size-4" />
                  <span>Restart Server</span>
                </DropdownMenuItem>
                <DropdownMenuItem
                  onClick={() => setShowShutdown(true)}
                  className="text-destructive focus:text-destructive"
                >
                  <PowerIcon className="size-4" />
                  <span>Shutdown Server</span>
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>

      {/* Restart Confirmation */}
      <AlertDialog open={showRestart} onOpenChange={setShowRestart}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Restart Server</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to restart the server? Active jobs will
              continue running.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleRestart}
              disabled={actionLoading}
            >
              {actionLoading ? "Restarting..." : "Restart"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Shutdown Confirmation */}
      <AlertDialog open={showShutdown} onOpenChange={setShowShutdown}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Shutdown Server</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to shut down the server? This will stop all
              scheduled jobs.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleShutdown}
              disabled={actionLoading}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {actionLoading ? "Shutting down..." : "Shutdown"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </SidebarRoot>
  );
}
