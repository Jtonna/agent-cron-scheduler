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
 * Destructive confirmation modal for deleting a job. The user must type
 * the job name verbatim to enable the confirm button — a soft tripwire
 * that prevents accidental destruction.
 *
 * Controlled by the parent: pass `isOpen` + `onOpenChange` and the dialog
 * tracks open state via React Aria's `Modal` overlay (focus trap + ESC
 * dismissal + click-outside come for free).
 *
 * `onConfirm` fires only when the typed name exactly matches `jobName`;
 * the parent is responsible for the actual delete + the resulting
 * navigation.
 */

interface DeleteJobDialogProps {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  jobName: string;
  onConfirm: () => void;
}

export function DeleteJobDialog({
  isOpen,
  onOpenChange,
  jobName,
  onConfirm,
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

  function handleConfirm() {
    if (!matches) return;
    onConfirm();
    handleOpenChange(false);
  }

  return (
    <ModalOverlay
      isOpen={isOpen}
      onOpenChange={handleOpenChange}
      isDismissable
      className="fixed inset-0 z-50 flex items-center justify-center bg-fg/30 backdrop-blur-sm entering:animate-in entering:fade-in exiting:animate-out exiting:fade-out"
    >
      <Modal className="bg-surface border border-status-failed-border rounded-card shadow-menu max-w-md w-full p-6 outline-none entering:animate-in entering:zoom-in-95 exiting:animate-out exiting:zoom-out-95">
        <Dialog className="outline-none flex flex-col gap-4">
          <Heading slot="title" className="text-fg text-lg font-semibold">
            Delete job?
          </Heading>

          <p className="text-fg-muted text-sm">
            This will permanently delete <strong className="text-fg">{jobName}</strong>, its run
            history, and any pending scheduled runs. This action cannot be undone.
          </p>

          <TextField
            value={typed}
            onChange={setTyped}
            className="flex flex-col gap-1.5"
            autoFocus
          >
            <Label className="text-xs font-medium text-fg-secondary">
              Type the job name to confirm
            </Label>
            <Input
              className="w-full px-3 py-2 text-sm bg-surface-secondary border border-border rounded-input outline-none focus:border-status-failed-border focus:ring-2 focus:ring-status-failed-border placeholder-fg-subtle"
              placeholder={jobName}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleConfirm();
              }}
            />
          </TextField>

          <div className="flex items-center justify-end gap-2 pt-2">
            <Button intent="ghost" size="sm" onPress={() => handleOpenChange(false)}>
              Cancel
            </Button>
            <Button
              intent="primary"
              size="sm"
              isDisabled={!matches}
              className="!bg-status-failed hover:!bg-status-failed/90"
              onPress={handleConfirm}
            >
              Delete job
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
