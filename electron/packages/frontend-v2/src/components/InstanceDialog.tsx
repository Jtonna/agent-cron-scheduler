"use client";

import { useState, useEffect } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { SavedConnection } from "@/lib/types";
import { saveConnection, updateConnection } from "@/lib/connections";
import {
  checkInstanceHealth,
  checkVersionCompatibility,
} from "@/hooks/useInstanceHealth";
import { getBaseUrl } from "@/lib/api";

interface InstanceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  // If provided, dialog is in "edit" mode with prefilled values
  editConnection?: SavedConnection | null;
  // Called after successfully saving a connection
  onSaved?: () => void;
}

function normalizeUrl(raw: string): string {
  let url = raw.trim().replace(/\/+$/, "");
  if (url && !/^https?:\/\//i.test(url)) {
    url = `http://${url}`;
  }
  return url;
}

export function InstanceDialog({
  open,
  onOpenChange,
  editConnection,
  onSaved,
}: InstanceDialogProps) {
  const isEditMode = !!editConnection;

  const [label, setLabel] = useState("");
  const [url, setUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  // Prefill fields when in edit mode or when dialog opens
  useEffect(() => {
    if (open) {
      if (editConnection) {
        setLabel(editConnection.label);
        setUrl(editConnection.url);
      } else {
        setLabel("");
        setUrl("");
      }
      setError(null);
    }
  }, [open, editConnection]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!label.trim()) {
      setError("Name is required.");
      return;
    }

    if (!url.trim()) {
      setError("URL is required.");
      return;
    }

    const normalizedUrl = normalizeUrl(url);

    setLoading(true);
    setError(null);

    try {
      // Check reachability of the remote instance
      const remoteHealth = await checkInstanceHealth(normalizedUrl);
      if (!remoteHealth.reachable) {
        setError(
          `Cannot reach instance: ${remoteHealth.error ?? "Unknown error"}`
        );
        setLoading(false);
        return;
      }

      // Check version compatibility against local instance
      const localUrl = getBaseUrl() || window.location.origin;
      const localHealth = await checkInstanceHealth(localUrl);

      if (localHealth.reachable && localHealth.version && remoteHealth.version) {
        const compatibility = checkVersionCompatibility(
          localHealth.version,
          remoteHealth.version
        );
        if (!compatibility.compatible) {
          setError(`Version incompatible: ${compatibility.message}`);
          setLoading(false);
          return;
        }
      }

      // Save the connection
      if (isEditMode && editConnection) {
        updateConnection(editConnection.id, { label: label.trim(), url: normalizedUrl });
      } else {
        const newConnection: SavedConnection = {
          id: crypto.randomUUID(),
          label: label.trim(),
          url: normalizedUrl,
          addedAt: new Date().toISOString(),
        };
        saveConnection(newConnection);
      }

      onSaved?.();
      onOpenChange(false);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "An unexpected error occurred."
      );
    } finally {
      setLoading(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {isEditMode ? "Edit Connection" : "Add Connection"}
          </DialogTitle>
          <DialogDescription>
            {isEditMode
              ? "Update the details for this remote ACS instance."
              : "Connect to a remote ACS instance by providing its URL."}
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="instance-name">Name</Label>
            <Input
              id="instance-name"
              type="text"
              placeholder="e.g. Desktop, Pi, VPS"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              disabled={loading}
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="instance-url">URL</Label>
            <Input
              id="instance-url"
              type="text"
              placeholder="e.g. http://192.168.1.100:3040"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              disabled={loading}
            />
          </div>

          {error && (
            <p className="text-sm text-destructive" role="alert">
              {error}
            </p>
          )}

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={loading}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={loading}>
              {loading
                ? "Validating..."
                : isEditMode
                ? "Save Changes"
                : "Add Connection"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
