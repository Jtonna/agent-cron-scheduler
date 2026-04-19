"use client";

import { useState } from "react";
import { useTheme } from "next-themes";
import { toast } from "sonner";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Tabs,
  TabsList,
  TabsTrigger,
  TabsContent,
} from "@/components/ui/tabs";
import { Button } from "@/components/ui/button";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Separator } from "@/components/ui/separator";
import { useAccentTheme } from "@/hooks/use-accent-theme";
import { useConnectionStatus } from "@/hooks/useConnectionStatus";
import { useHealth } from "@/hooks/useHealth";
import { api, getBaseUrl } from "@/lib/api";
import { getActiveConnection } from "@/lib/connections";
import { formatUptime } from "@/lib/format";
import { cn } from "@/lib/utils";

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

// ── Appearance Section ────────────────────────────────────────────────────────

function AppearanceSection() {
  const { theme, setTheme } = useTheme();
  const { accent, setAccent } = useAccentTheme();

  const themes = [
    { value: "light", label: "Light" },
    { value: "dark", label: "Dark" },
    { value: "system", label: "System" },
  ] as const;

  return (
    <div className="space-y-6">
      {/* Theme selector */}
      <div className="space-y-3">
        <p className="text-sm font-medium">Theme</p>
        <div className="flex gap-2">
          {themes.map((t) => (
            <Button
              key={t.value}
              variant={theme === t.value ? "default" : "outline"}
              size="sm"
              onClick={() => setTheme(t.value)}
              className="flex-1"
            >
              {t.label}
            </Button>
          ))}
        </div>
      </div>

      <Separator />

      {/* Accent color selector */}
      <div className="space-y-3">
        <p className="text-sm font-medium">Accent Color</p>
        <div className="flex items-center gap-3">
          {/* Burgundy swatch */}
          <button
            type="button"
            aria-label="Burgundy accent"
            onClick={() => setAccent("burgundy")}
            className={cn(
              "h-8 w-8 rounded-full bg-[oklch(0.50_0.155_18)] transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
              accent === "burgundy" && "ring-2 ring-ring ring-offset-2"
            )}
          />
          <span
            className={cn(
              "text-sm",
              accent === "burgundy" ? "text-foreground font-medium" : "text-muted-foreground"
            )}
          >
            Burgundy
          </span>

          <div className="w-6" />

          {/* Sage swatch */}
          <button
            type="button"
            aria-label="Sage accent"
            onClick={() => setAccent("sage")}
            className={cn(
              "h-8 w-8 rounded-full bg-[oklch(0.44_0.110_152)] transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
              accent === "sage" && "ring-2 ring-ring ring-offset-2"
            )}
          />
          <span
            className={cn(
              "text-sm",
              accent === "sage" ? "text-foreground font-medium" : "text-muted-foreground"
            )}
          >
            Sage
          </span>
        </div>
      </div>
    </div>
  );
}

// ── Connection Section ────────────────────────────────────────────────────────

function ConnectionSection() {
  const connectionState = useConnectionStatus();
  const baseUrl = getBaseUrl();
  const activeConnection = getActiveConnection();
  const connectionName = activeConnection?.label ?? "Local";

  const statusConfig = {
    connected: { dot: "bg-green-500", label: "Connected" },
    connecting: { dot: "bg-yellow-500", label: "Connecting" },
    offline: { dot: "bg-red-500", label: "Offline" },
  } as const;

  const { dot, label } = statusConfig[connectionState];

  return (
    <div className="space-y-6">
      {/* Current connection info */}
      <div className="space-y-4">
        <p className="text-sm font-medium">Current Connection</p>

        <div className="rounded-lg border bg-muted/30 p-4 space-y-3">
          <div className="flex items-center justify-between text-sm">
            <span className="text-muted-foreground">Name</span>
            <span className="font-medium">{connectionName}</span>
          </div>

          <Separator />

          <div className="flex items-start justify-between text-sm gap-4">
            <span className="text-muted-foreground shrink-0">URL</span>
            <span className="font-mono text-xs break-all text-right">
              {baseUrl || "—"}
            </span>
          </div>

          <Separator />

          <div className="flex items-center justify-between text-sm">
            <span className="text-muted-foreground">Status</span>
            <div className="flex items-center gap-2">
              <span className={cn("h-2 w-2 rounded-full", dot)} />
              <span className="font-medium">{label}</span>
            </div>
          </div>
        </div>
      </div>

      <Separator />

      {/* Placeholder */}
      <div className="rounded-lg border border-dashed p-4 text-center">
        <p className="text-sm text-muted-foreground">
          Remote instance management coming soon
        </p>
      </div>
    </div>
  );
}

// ── System Section ────────────────────────────────────────────────────────────

function SystemSection() {
  const [restartLoading, setRestartLoading] = useState(false);
  const [shutdownLoading, setShutdownLoading] = useState(false);

  const handleRestart = async () => {
    setRestartLoading(true);
    try {
      await api.restart();
      toast.success("Server restarting...");
    } catch (err) {
      toast.error(
        err instanceof Error ? err.message : "Failed to restart server"
      );
    } finally {
      setRestartLoading(false);
    }
  };

  const handleShutdown = async () => {
    setShutdownLoading(true);
    try {
      await api.shutdown();
      toast.success("Server shutting down...");
    } catch (err) {
      toast.error(
        err instanceof Error ? err.message : "Failed to shut down server"
      );
    } finally {
      setShutdownLoading(false);
    }
  };

  return (
    <div className="space-y-4">
      {/* Restart */}
      <div className="flex items-center justify-between rounded-lg border p-4">
        <div className="space-y-1">
          <p className="text-sm font-medium">Restart Server</p>
          <p className="text-xs text-muted-foreground">
            Restart the ACS daemon process.
          </p>
        </div>
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button variant="outline" size="sm" disabled={restartLoading}>
              {restartLoading ? "Restarting..." : "Restart"}
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Restart Server?</AlertDialogTitle>
              <AlertDialogDescription>
                The ACS daemon will restart. Running jobs may be interrupted
                briefly.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Cancel</AlertDialogCancel>
              <AlertDialogAction onClick={handleRestart}>
                Restart
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>

      {/* Shutdown */}
      <div className="flex items-center justify-between rounded-lg border border-destructive/30 p-4">
        <div className="space-y-1">
          <p className="text-sm font-medium text-destructive">Shutdown Server</p>
          <p className="text-xs text-muted-foreground">
            Stop the ACS daemon completely.
          </p>
        </div>
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button variant="destructive" size="sm" disabled={shutdownLoading}>
              {shutdownLoading ? "Shutting down..." : "Shutdown"}
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Shutdown Server?</AlertDialogTitle>
              <AlertDialogDescription>
                The ACS daemon will stop. All scheduled jobs will stop running
                until the server is restarted manually.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Cancel</AlertDialogCancel>
              <AlertDialogAction
                className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                onClick={handleShutdown}
              >
                Shutdown
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
    </div>
  );
}

// ── About Section ─────────────────────────────────────────────────────────────

function AboutSection() {
  const { health, loading } = useHealth();

  const dash = "—";

  const version = loading ? dash : health?.version ?? dash;
  const dataDir = loading ? dash : health?.data_dir ?? dash;
  const uptime =
    loading || health?.uptime_seconds == null
      ? dash
      : formatUptime(health.uptime_seconds);

  return (
    <div className="space-y-6">
      <div className="rounded-lg border bg-muted/30 p-4 space-y-3">
        <div className="flex items-center justify-between text-sm">
          <span className="text-muted-foreground">Version</span>
          <span className="font-mono text-xs">{version}</span>
        </div>

        <Separator />

        <div className="flex items-start justify-between text-sm gap-4">
          <span className="text-muted-foreground shrink-0">Data Directory</span>
          <span className="font-mono text-xs break-all text-right max-w-[220px] truncate" title={dataDir}>
            {dataDir}
          </span>
        </div>

        <Separator />

        <div className="flex items-center justify-between text-sm">
          <span className="text-muted-foreground">Uptime</span>
          <span className="font-mono text-xs">{uptime}</span>
        </div>
      </div>

      <div className="flex justify-center">
        <a
          href="https://github.com/Jtonna/agent-cron-scheduler"
          target="_blank"
          rel="noopener noreferrer"
          className="text-sm text-primary underline-offset-4 hover:underline"
        >
          View on GitHub
        </a>
      </div>
    </div>
  );
}

// ── SettingsDialog ────────────────────────────────────────────────────────────

export function SettingsDialog({ open, onOpenChange }: SettingsDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg rounded-2xl">
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
        </DialogHeader>

        <Tabs defaultValue="appearance" className="w-full">
          <TabsList className="w-full grid grid-cols-4">
            <TabsTrigger value="appearance">Appearance</TabsTrigger>
            <TabsTrigger value="connection">Connection</TabsTrigger>
            <TabsTrigger value="system">System</TabsTrigger>
            <TabsTrigger value="about">About</TabsTrigger>
          </TabsList>

          <TabsContent value="appearance" className="mt-4">
            <AppearanceSection />
          </TabsContent>

          <TabsContent value="connection" className="mt-4">
            <ConnectionSection />
          </TabsContent>

          <TabsContent value="system" className="mt-4">
            <SystemSection />
          </TabsContent>

          <TabsContent value="about" className="mt-4">
            <AboutSection />
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
