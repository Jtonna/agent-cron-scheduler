"use client";

import { useState } from "react";
import {
  Dialog,
  Heading,
  Input,
  Label,
  Modal,
  ModalOverlay,
  TextField,
} from "react-aria-components";
import { Button } from "@/components/ui/Button";

/**
 * DeleteJobDialog
 *
 * Destructive confirmation modal for deleting a workflow. The user must
 * type the workflow name exactly to enable the confirm button — a soft
 * tripwire that prevents accidental destruction.
 *
 * Controlled by the parent: pass `isOpen` + `onOpenChange` and the dialog
 * tracks open state via React Aria's `Modal` overlay (focus trap + ESC
 * dismissal + click-outside come for free).
 *
 * `onConfirm` fires only when the typed name exactly matches `jobName`.
 * It may return void or a Promise: when it returns a Promise, the dialog
 * awaits it, stays open and non-dismissable while pending, and only closes
 * itself on resolution. On rejection the dialog stays open (and the typed
 * confirmation is preserved) so the parent can surface `error` and let the
 * user retry. The parent owns the actual delete call, the pending/error
 * state passed back in via props, and any resulting navigation.
 */

interface DeleteJobDialogProps {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  jobName: string;
  onConfirm: () => void | Promise<void>;
  isPending?: boolean;
  error?: string | null;
}

export function DeleteJobDialog({
  isOpen,
  onOpenChange,
  jobName,
  onConfirm,
  isPending = false,
  error = null,
}: DeleteJobDialogProps) {
  const [typed, setTyped] = useState("");
  const matches = typed === jobName;

  // Reset typed confirmation when the modal closes (or is asked to close).
  // Doing it in the close callback avoids the useEffect-setState anti-pattern
  // and matches the natural lifecycle: when the modal closes, its state is
  // implicitly reset for next time.
  function handleOpenChange(open: boolean) {
    if (!open) setTyped("");
    onOpenChange(open);
  }

  async function handleConfirm() {
    if (isPending || !matches) return;
    try {
      await onConfirm();
      handleOpenChange(false);
    } catch {
      // Leave the dialog open with `typed` intact; the parent is expected
      // to surface the failure via the `error` prop so the user can retry.
    }
  }

  return (
    <ModalOverlay
      isOpen={isOpen}
      onOpenChange={handleOpenChange}
      isDismissable={!isPending}
      isKeyboardDismissDisabled={!!isPending}
      className="fixed inset-0 z-50 flex items-center justify-center bg-fg/30 backdrop-blur-sm entering:animate-in entering:fade-in exiting:animate-out exiting:fade-out"
    >
      <Modal className="bg-surface border border-status-failed-border rounded-card shadow-menu max-w-md w-full p-6 outline-none entering:animate-in entering:zoom-in-95 exiting:animate-out exiting:zoom-out-95">
        <Dialog className="outline-none flex flex-col gap-4">
          <Heading slot="title" className="text-fg text-lg font-semibold">
            Delete workflow?
          </Heading>

          <p className="text-fg-muted text-sm">
            <strong className="text-fg">{jobName}</strong> will be removed from your workflows
            and its schedule stopped. Past runs and cost history are kept.
          </p>

          <TextField
            value={typed}
            onChange={setTyped}
            className="flex flex-col gap-1.5"
            autoFocus
            isDisabled={isPending}
          >
            <Label className="text-xs font-medium text-fg-secondary">
              Type the workflow name to confirm
            </Label>
            <Input
              className="w-full px-3 py-2 text-sm bg-surface-secondary border border-border rounded-input outline-none focus:border-status-failed-border focus:ring-2 focus:ring-status-failed-border placeholder-fg-subtle"
              placeholder={jobName}
              onKeyDown={(e) => {
                if (e.key === "Enter") void handleConfirm();
              }}
            />
          </TextField>

          {error && (
            <div
              role="alert"
              className="p-3 text-sm bg-status-failed-bg border border-status-failed-border text-status-failed rounded-card"
            >
              {error}
            </div>
          )}

          <div className="flex items-center justify-end gap-2 pt-2">
            <Button
              intent="ghost"
              size="sm"
              onPress={() => handleOpenChange(false)}
              isDisabled={isPending}
            >
              Cancel
            </Button>
            <Button
              intent="primary"
              size="sm"
              isDisabled={!matches || isPending}
              className="!bg-status-failed hover:!bg-status-failed/90"
              onPress={() => void handleConfirm()}
            >
              {isPending ? "Deleting…" : "Delete workflow"}
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
