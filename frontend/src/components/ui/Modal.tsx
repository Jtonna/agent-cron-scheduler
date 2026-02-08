"use client";

import React, { useEffect } from "react";
import { Dialog, Row } from "@once-ui-system/core";

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title: string;
  children: React.ReactNode;
  actions?: React.ReactNode;
}

export function Modal({ open, onClose, title, children, actions }: ModalProps) {
  useEffect(() => {
    if (!open) return;

    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [open, onClose]);

  return (
    <Dialog
      isOpen={open}
      onClose={onClose}
      title={title}
      footer={
        actions ? (
          <Row gap="8" horizontal="end">
            {actions}
          </Row>
        ) : undefined
      }
    >
      {children}
    </Dialog>
  );
}
