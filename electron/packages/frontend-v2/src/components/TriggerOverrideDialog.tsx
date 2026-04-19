"use client";

import { useState } from "react";
import { toast } from "sonner";
import { MinusIcon } from "@heroicons/react/24/outline";
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
import { Textarea } from "@/components/ui/textarea";
import { Separator } from "@/components/ui/separator";
import { api } from "@/lib/api";
import type { TriggerParams } from "@/lib/types";

interface TriggerOverrideDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  jobId: string;
  jobName: string;
  onTriggered: (runId: string) => void;
}

interface EnvRow {
  key: string;
  value: string;
}

function resetForm() {
  return {
    args: "",
    envRows: [] as EnvRow[],
    stdin: "",
  };
}

export function TriggerOverrideDialog({
  open,
  onOpenChange,
  jobId,
  jobName,
  onTriggered,
}: TriggerOverrideDialogProps) {
  const [args, setArgs] = useState("");
  const [envRows, setEnvRows] = useState<EnvRow[]>([]);
  const [stdin, setStdin] = useState("");
  const [loading, setLoading] = useState(false);

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) {
      const { args: a, envRows: e, stdin: s } = resetForm();
      setArgs(a);
      setEnvRows(e);
      setStdin(s);
    }
    onOpenChange(nextOpen);
  };

  const addEnvRow = () => {
    setEnvRows((prev) => [...prev, { key: "", value: "" }]);
  };

  const removeEnvRow = (index: number) => {
    setEnvRows((prev) => prev.filter((_, i) => i !== index));
  };

  const updateEnvRow = (index: number, field: "key" | "value", val: string) => {
    setEnvRows((prev) => {
      const updated = [...prev];
      updated[index] = { ...updated[index], [field]: val };
      return updated;
    });
  };

  const handleTrigger = async () => {
    setLoading(true);
    try {
      const params: TriggerParams = {};

      if (args.trim()) {
        params.args = args.trim();
      }

      const validEnvPairs = envRows.filter((row) => row.key.trim() !== "");
      if (validEnvPairs.length > 0) {
        params.env = validEnvPairs.reduce<Record<string, string>>(
          (acc, row) => {
            acc[row.key.trim()] = row.value;
            return acc;
          },
          {}
        );
      }

      if (stdin.trim()) {
        params.input = stdin.trim();
      }

      const result = await api.triggerJob(
        jobId,
        Object.keys(params).length > 0 ? params : undefined
      );
      toast.success(`Job "${jobName}" triggered`);
      handleOpenChange(false);
      onTriggered(result.run_id);
    } catch (err) {
      toast.error(
        err instanceof Error ? err.message : "Failed to trigger job"
      );
    } finally {
      setLoading(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-lg rounded-2xl">
        <DialogHeader>
          <DialogTitle>Trigger {jobName}</DialogTitle>
          <DialogDescription>
            Optionally override arguments, environment variables, or stdin for
            this run only.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-5 py-2">
          {/* Command args override */}
          <div className="space-y-2">
            <Label htmlFor="trigger-args">Command Arguments</Label>
            <Input
              id="trigger-args"
              value={args}
              onChange={(e) => setArgs(e.target.value)}
              placeholder="Override command arguments..."
              autoComplete="off"
              spellCheck={false}
            />
          </div>

          <Separator />

          {/* Environment variable overrides */}
          <div className="space-y-2">
            <Label>Environment Variable Overrides</Label>
            {envRows.length === 0 && (
              <p className="text-sm text-muted-foreground py-1">
                No overrides added.
              </p>
            )}
            <div className="space-y-2">
              {envRows.map((row, index) => (
                <div key={index} className="flex items-center gap-2">
                  <Input
                    placeholder="KEY"
                    value={row.key}
                    onChange={(e) => updateEnvRow(index, "key", e.target.value)}
                    className="font-mono"
                    autoComplete="off"
                    spellCheck={false}
                  />
                  <span className="text-muted-foreground shrink-0" aria-hidden="true">
                    =
                  </span>
                  <Input
                    placeholder="value"
                    value={row.value}
                    onChange={(e) =>
                      updateEnvRow(index, "value", e.target.value)
                    }
                    className="font-mono"
                    autoComplete="off"
                    spellCheck={false}
                  />
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    className="h-9 w-9 shrink-0"
                    onClick={() => removeEnvRow(index)}
                    aria-label={`Remove variable ${row.key || index}`}
                  >
                    <MinusIcon className="h-4 w-4" />
                  </Button>
                </div>
              ))}
            </div>
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={addEnvRow}
            >
              + Add Variable
            </Button>
          </div>

          <Separator />

          {/* Stdin override */}
          <div className="space-y-2">
            <Label htmlFor="trigger-stdin">Standard Input</Label>
            <Textarea
              id="trigger-stdin"
              value={stdin}
              onChange={(e) => setStdin(e.target.value)}
              placeholder="Standard input..."
              rows={3}
              spellCheck={false}
            />
          </div>
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => handleOpenChange(false)}
            disabled={loading}
          >
            Cancel
          </Button>
          <Button onClick={handleTrigger} disabled={loading}>
            {loading ? "Triggering..." : "Trigger"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
