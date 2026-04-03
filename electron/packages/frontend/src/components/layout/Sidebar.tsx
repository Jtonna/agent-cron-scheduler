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
  ChevronUpDownIcon,
  PlusIcon,
  PencilIcon,
  TrashIcon,
  StopIcon,
  ExclamationTriangleIcon,
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
import type { Job, SavedConnection, RecentRunEntry } from "@/lib/types";
import { formatDate } from "@/lib/format";
import { useSSEEvents } from "@/hooks/useSSE";
import { getConnections, getActiveConnectionId, setActiveConnectionId, deleteConnection } from "@/lib/connections";
import { InstanceDialog } from "@/components/InstanceDialog";

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
  const [connections, setConnections] = useState<SavedConnection[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [editingConnection, setEditingConnection] = useState<SavedConnection | null>(null);
  const [deletingConnection, setDeletingConnection] = useState<SavedConnection | null>(null);

  // Refresh connections from localStorage
  const refreshConnections = useCallback(() => {
    setConnections(getConnections());
    setActiveId(getActiveConnectionId());
  }, []);

  useEffect(() => {
    refreshConnections();
  }, [refreshConnections]);

  const handleSwitchInstance = (connectionId: string | null) => {
    setActiveConnectionId(connectionId);
    // Full page reload to re-init SSE and all API calls with the new base URL
    window.location.href = "/";
  };

  const handleDeleteConnection = (conn: SavedConnection) => {
    deleteConnection(conn.id);
    refreshConnections();
    setDeletingConnection(null);
    // If the deleted connection was active, reload to go back to local
    if (activeId === conn.id) {
      window.location.href = "/";
    }
  };

  // Compute the active instance name for dialog messages
  const instanceName = activeId
    ? connections.find((c) => c.id === activeId)?.label ?? "Remote"
    : "Local";

  const [recentRuns, setRecentRuns] = useState<RecentRunEntry[]>([]);
  const [recentRunsError, setRecentRunsError] = useState(false);

  const fetchJobs = useCallback(async () => {
    try {
      const data = await api.listJobs();
      if (Array.isArray(data)) {
        setJobs(data);
      }
    } catch {
      // Ignore errors silently in sidebar
    }
  }, []);

  const fetchRecentRuns = useCallback(async () => {
    try {
      setRecentRunsError(false);
      const data = await api.listRecentRuns(20);
      if (data && Array.isArray(data.runs)) {
        setRecentRuns(data.runs);
      }
    } catch {
      setRecentRunsError(true);
    }
  }, []);

  useEffect(() => {
    fetchJobs();
    fetchRecentRuns();
    const timer = setInterval(() => {
      fetchJobs();
      fetchRecentRuns();
    }, 10000);
    return () => clearInterval(timer);
  }, [fetchJobs, fetchRecentRuns]);

  useSSEEvents(
    useCallback(
      (event) => {
        if (
          event.type === "job_changed" ||
          event.type === "completed" ||
          event.type === "failed" ||
          event.type === "started" ||
          event.type === "killed"
        ) {
          fetchJobs();
          fetchRecentRuns();
        }
      },
      [fetchJobs, fetchRecentRuns]
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
        { label: "Monitoring", icon: HomeIcon, path: "/" },
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

  // Group recent runs by job, keeping all Running + up to 2 most recent non-running per job
  const groupedRecentRuns = React.useMemo(() => {
    const byJob = new Map<string, { jobName: string; jobId: string; runs: RecentRunEntry[] }>();
    for (const run of recentRuns) {
      if (!byJob.has(run.job_id)) {
        byJob.set(run.job_id, { jobName: run.job_name, jobId: run.job_id, runs: [] });
      }
      byJob.get(run.job_id)!.runs.push(run);
    }

    const groups = Array.from(byJob.values()).map((group) => {
      const running = group.runs.filter((r) => r.status === "Running");
      const nonRunning = group.runs
        .filter((r) => r.status !== "Running")
        .sort((a, b) => new Date(b.started_at).getTime() - new Date(a.started_at).getTime())
        .slice(0, 2);
      return {
        ...group,
        runs: [...running, ...nonRunning].sort(
          (a, b) => new Date(b.started_at).getTime() - new Date(a.started_at).getTime()
        ),
      };
    });

    // Sort groups: jobs with Running runs first, then by most recent started_at
    groups.sort((a, b) => {
      const aHasRunning = a.runs.some((r) => r.status === "Running") ? 1 : 0;
      const bHasRunning = b.runs.some((r) => r.status === "Running") ? 1 : 0;
      if (bHasRunning !== aHasRunning) return bHasRunning - aHasRunning;
      const aMax = Math.max(...a.runs.map((r) => new Date(r.started_at).getTime()));
      const bMax = Math.max(...b.runs.map((r) => new Date(r.started_at).getTime()));
      return bMax - aMax;
    });

    return groups;
  }, [recentRuns]);

  function runStatusIcon(status: RecentRunEntry["status"]) {
    switch (status) {
      case "Running":
        return <ArrowPathIcon className="size-3.5 text-blue-500 animate-spin shrink-0" />;
      case "Completed":
        return <CheckCircleSolid className="size-3.5 text-emerald-600 dark:text-emerald-400 shrink-0" />;
      case "CompletedWithWarnings":
        return <ExclamationTriangleIcon className="size-3.5 text-amber-500 dark:text-amber-400 shrink-0" />;
      case "Failed":
        return <XCircleSolid className="size-3.5 text-red-600 dark:text-red-400 shrink-0" />;
      case "Killed":
        return <StopIcon className="size-3.5 text-red-500/70 dark:text-red-400/70 shrink-0" />;
      default:
        return <ClockIcon className="size-3.5 text-muted-foreground shrink-0" />;
    }
  }

  return (
    <SidebarRoot collapsible="offcanvas">
      {/* Header - Instance Switcher */}
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <SidebarMenuButton size="lg">
                  <div className="flex aspect-square size-8 items-center justify-center rounded-lg bg-primary text-primary-foreground">
                    <CommandLineIcon className="size-4" />
                  </div>
                  <div className="grid flex-1 text-left text-sm leading-tight">
                    <span className="truncate font-semibold">
                      {activeId
                        ? connections.find((c) => c.id === activeId)?.label ?? "Remote"
                        : "Local"}
                    </span>
                    <span className="truncate text-xs text-muted-foreground">
                      ACS Instance
                    </span>
                  </div>
                  <ChevronUpDownIcon className="ml-auto size-4" />
                </SidebarMenuButton>
              </DropdownMenuTrigger>
              <DropdownMenuContent
                className="w-[--radix-popper-anchor-width] min-w-56"
                align="start"
                side="bottom"
                sideOffset={4}
              >
                {/* Local instance - always first */}
                <DropdownMenuItem onClick={() => handleSwitchInstance(null)}>
                  <span className="inline-block size-2 rounded-full bg-emerald-500 mr-2 shrink-0" />
                  <span className="flex-1">Local</span>
                  {!activeId && <span className="text-xs text-muted-foreground ml-2">&#10003;</span>}
                </DropdownMenuItem>

                {connections.length > 0 && <DropdownMenuSeparator />}

                {connections.map((conn) => (
                  <DropdownMenuItem
                    key={conn.id}
                    onClick={() => handleSwitchInstance(conn.id)}
                    className="group"
                  >
                    <span className="inline-block size-2 rounded-full bg-muted-foreground/50 mr-2 shrink-0" />
                    <span className="flex-1 truncate">{conn.label}</span>
                    {activeId === conn.id && (
                      <span className="text-xs text-muted-foreground ml-1">&#10003;</span>
                    )}
                    <span className="ml-1 flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          setEditingConnection(conn);
                        }}
                        className="p-0.5 rounded hover:bg-accent"
                      >
                        <PencilIcon className="size-3" />
                      </button>
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          setDeletingConnection(conn);
                        }}
                        className="p-0.5 rounded hover:bg-destructive/20 text-destructive"
                      >
                        <TrashIcon className="size-3" />
                      </button>
                    </span>
                  </DropdownMenuItem>
                ))}

                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={() => setShowAddDialog(true)}>
                  <PlusIcon className="size-4 mr-2" />
                  <span>Add Connection...</span>
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
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

        {/* Recent Runs — grouped by job */}
        <Collapsible defaultOpen className="group/collapsible">
          <SidebarGroup>
            <SidebarGroupLabel asChild>
              <CollapsibleTrigger>
                Recent Runs
                <ChevronRightIcon className="ml-auto size-4 transition-transform duration-200 group-data-[state=open]/collapsible:rotate-90" />
              </CollapsibleTrigger>
            </SidebarGroupLabel>
            <CollapsibleContent>
              <SidebarGroupContent>
                <SidebarMenu>
                  {groupedRecentRuns.length === 0 ? (
                    <SidebarMenuItem>
                      <SidebarMenuButton className="pointer-events-none text-sidebar-foreground/50">
                        {recentRunsError ? (
                          <span className="flex items-center gap-1 text-xs text-amber-500/70 dark:text-amber-400/60">
                            <ExclamationTriangleIcon className="size-3 shrink-0" />
                            Unable to load
                          </span>
                        ) : (
                          <span className="text-xs">No recent runs</span>
                        )}
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  ) : (
                    groupedRecentRuns.map((group) => (
                      <Collapsible key={group.jobId} defaultOpen className="group/job">
                        <SidebarMenuItem>
                          <SidebarMenuButton
                            tooltip={group.jobName}
                            isActive={pathname === `/jobs/${group.jobId}`}
                            className="pr-1"
                          >
                            <ClockIcon className="size-4 shrink-0" />
                            <Link
                              to={`/jobs/${group.jobId}`}
                              className="truncate font-medium flex-1 hover:text-foreground hover:underline"
                              onClick={(e) => e.stopPropagation()}
                            >
                              {group.jobName}
                            </Link>
                            <CollapsibleTrigger asChild>
                              <button
                                type="button"
                                className="ml-auto shrink-0 p-0.5 rounded hover:bg-accent"
                                aria-label="Toggle runs"
                              >
                                <ChevronRightIcon className="size-3.5 transition-transform duration-200 group-data-[state=open]/job:rotate-90" />
                              </button>
                            </CollapsibleTrigger>
                          </SidebarMenuButton>
                          <CollapsibleContent>
                            <SidebarMenuSub>
                              {group.runs.map((run) => (
                                <SidebarMenuSubItem key={run.run_id}>
                                  <SidebarMenuSubButton
                                    asChild
                                    isActive={pathname === `/jobs/${run.job_id}/runs/${run.run_id}`}
                                  >
                                    <Link to={`/jobs/${run.job_id}/runs/${run.run_id}`}>
                                      {runStatusIcon(run.status)}
                                      <span className="font-mono text-xs truncate">
                                        {run.run_id.substring(0, 8)}&hellip;
                                      </span>
                                      <span className="ml-auto text-[10px] text-muted-foreground whitespace-nowrap">
                                        {formatDate(run.started_at)}
                                      </span>
                                    </Link>
                                  </SidebarMenuSubButton>
                                </SidebarMenuSubItem>
                              ))}
                            </SidebarMenuSub>
                          </CollapsibleContent>
                        </SidebarMenuItem>
                      </Collapsible>
                    ))
                  )}
                </SidebarMenu>
              </SidebarGroupContent>
            </CollapsibleContent>
          </SidebarGroup>
        </Collapsible>
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
              {activeId
                ? `Are you sure you want to restart ${instanceName}? Active jobs will continue running.`
                : "Are you sure you want to restart the server? Active jobs will continue running."}
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
              {activeId
                ? `Are you sure you want to shut down ${instanceName}? This will stop all scheduled jobs.`
                : "Are you sure you want to shut down the server? This will stop all scheduled jobs."}
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
      <InstanceDialog
        open={showAddDialog}
        onOpenChange={setShowAddDialog}
        onSaved={refreshConnections}
      />
      <InstanceDialog
        open={!!editingConnection}
        onOpenChange={(open) => {
          if (!open) setEditingConnection(null);
        }}
        editConnection={editingConnection}
        onSaved={refreshConnections}
      />
      <AlertDialog open={!!deletingConnection} onOpenChange={(open) => { if (!open) setDeletingConnection(null); }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove Connection</AlertDialogTitle>
            <AlertDialogDescription>
              {deletingConnection && activeId === deletingConnection.id
                ? `"${deletingConnection?.label}" is the active connection. Removing it will switch back to the local instance.`
                : `Are you sure you want to remove "${deletingConnection?.label}"?`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => deletingConnection && handleDeleteConnection(deletingConnection)}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              Remove
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </SidebarRoot>
  );
}
