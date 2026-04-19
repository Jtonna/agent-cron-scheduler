"use client";

import { useState, useEffect, useCallback } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import {
  ChevronDownIcon,
  PlusIcon,
  PencilIcon,
  TrashIcon,
  Cog6ToothIcon,
  SunIcon,
  MoonIcon,
  CheckIcon,
} from "@heroicons/react/24/outline";
import { useTheme } from "next-themes";
import { useAccentTheme } from "@/hooks/use-accent-theme";
import type { SavedConnection } from "@/lib/types";
import {
  getConnections,
  getActiveConnectionId,
  setActiveConnectionId,
  deleteConnection,
} from "@/lib/connections";
import { PillToggle } from "@/components/ui/pill-toggle";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
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
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { InstanceDialog } from "@/components/InstanceDialog";
import { SettingsDialog } from "@/components/SettingsDialog";

const NAV_OPTIONS = [
  { label: "Dashboard", value: "dashboard" },
  { label: "Jobs", value: "jobs" },
] as const;

export function TopBar() {
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const { theme, setTheme } = useTheme();
  const { accent, setAccent } = useAccentTheme();

  // Connection state
  const [connections, setConnections] = useState<SavedConnection[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [editingConnection, setEditingConnection] =
    useState<SavedConnection | null>(null);
  const [deletingConnection, setDeletingConnection] =
    useState<SavedConnection | null>(null);

  // Settings dialog state
  const [showSettings, setShowSettings] = useState(false);

  // Refresh connections from localStorage
  const refreshConnections = useCallback(() => {
    setConnections(getConnections());
    setActiveId(getActiveConnectionId());
  }, []);

  useEffect(() => {
    refreshConnections();
  }, [refreshConnections]);

  // Derive active pill value from current route
  const activeNav = pathname.startsWith("/jobs") ? "jobs" : "dashboard";

  const handleNavChange = (value: string) => {
    if (value === "jobs") {
      navigate("/jobs");
    } else {
      navigate("/");
    }
  };

  // Connection management
  const handleSwitchInstance = (connectionId: string | null) => {
    setActiveConnectionId(connectionId);
    window.location.reload();
  };

  const handleDeleteConnection = (conn: SavedConnection) => {
    deleteConnection(conn.id);
    refreshConnections();
    setDeletingConnection(null);
    if (activeId === conn.id) {
      window.location.href = "/";
    }
  };

  // Compute instance name for display
  const instanceName = activeId
    ? connections.find((c) => c.id === activeId)?.label ?? "Remote"
    : "Local";

  return (
    <>
      <header
        className="fixed top-0 left-0 right-0 z-50 flex h-[49px] items-center justify-between bg-background/80 backdrop-blur-md border-b border-border/50 px-4"
        style={{ "--topbar-height": "49px" } as React.CSSProperties}
      >
        {/* Left section: Instance/Connection Switcher */}
        <div className="flex items-center">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="sm" className="gap-1.5 font-medium">
                <span className="inline-block size-2 rounded-full bg-emerald-500 shrink-0" />
                <span className="truncate max-w-[160px]">{instanceName}</span>
                <ChevronDownIcon className="size-3.5 text-muted-foreground" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-64">
              {/* Local instance - always first */}
              <DropdownMenuItem onClick={() => handleSwitchInstance(null)}>
                <span className="inline-block size-2 rounded-full bg-emerald-500 mr-2 shrink-0" />
                <span className="flex-1">Local</span>
                {!activeId && (
                  <CheckIcon className="size-4 ml-2 text-muted-foreground" />
                )}
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
                    <CheckIcon className="size-4 ml-1 text-muted-foreground" />
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
        </div>

        {/* Center section: Pill Toggle */}
        <div className="absolute left-1/2 -translate-x-1/2">
          <PillToggle
            options={[...NAV_OPTIONS]}
            value={activeNav}
            onChange={handleNavChange}
          />
        </div>

        {/* Right section: Theme controls + Settings */}
        <div className="flex items-center gap-1.5">
          {/* Accent swatches */}
          <TooltipProvider delayDuration={300}>
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  onClick={() => setAccent("burgundy")}
                  className={`size-5 rounded-full bg-[oklch(0.45_0.12_18)] transition-all cursor-pointer ${
                    accent === "burgundy"
                      ? "ring-2 ring-primary ring-offset-2 ring-offset-background"
                      : "hover:ring-1 hover:ring-border"
                  }`}
                  aria-label="Burgundy accent theme"
                />
              </TooltipTrigger>
              <TooltipContent>Burgundy</TooltipContent>
            </Tooltip>

            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  onClick={() => setAccent("sage")}
                  className={`size-5 rounded-full bg-[oklch(0.55_0.10_148)] transition-all cursor-pointer ${
                    accent === "sage"
                      ? "ring-2 ring-primary ring-offset-2 ring-offset-background"
                      : "hover:ring-1 hover:ring-border"
                  }`}
                  aria-label="Sage accent theme"
                />
              </TooltipTrigger>
              <TooltipContent>Sage</TooltipContent>
            </Tooltip>

            {/* Theme toggle */}
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-8"
                  onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
                  aria-label={
                    theme === "dark"
                      ? "Switch to light mode"
                      : "Switch to dark mode"
                  }
                >
                  {theme === "dark" ? (
                    <SunIcon className="size-4" />
                  ) : (
                    <MoonIcon className="size-4" />
                  )}
                </Button>
              </TooltipTrigger>
              <TooltipContent>
                {theme === "dark" ? "Light mode" : "Dark mode"}
              </TooltipContent>
            </Tooltip>

            {/* Settings gear button */}
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="size-8"
                  aria-label="Settings"
                  onClick={() => setShowSettings(true)}
                >
                  <Cog6ToothIcon className="size-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>Settings</TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </div>
      </header>

      {/* Settings Dialog */}
      <SettingsDialog open={showSettings} onOpenChange={setShowSettings} />

      {/* Add Connection Dialog */}
      <InstanceDialog
        open={showAddDialog}
        onOpenChange={setShowAddDialog}
        onSaved={refreshConnections}
      />

      {/* Edit Connection Dialog */}
      <InstanceDialog
        open={!!editingConnection}
        onOpenChange={(open) => {
          if (!open) setEditingConnection(null);
        }}
        editConnection={editingConnection}
        onSaved={refreshConnections}
      />

      {/* Delete Connection Confirmation */}
      <AlertDialog
        open={!!deletingConnection}
        onOpenChange={(open) => {
          if (!open) setDeletingConnection(null);
        }}
      >
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
              onClick={() =>
                deletingConnection &&
                handleDeleteConnection(deletingConnection)
              }
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              Remove
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
