"use client";

import { useToast as useOnceToast } from "@once-ui-system/core";

type ToastVariant = "success" | "error" | "info";

const variantMap: Record<ToastVariant, "success" | "danger" | "info"> = {
  success: "success",
  error: "danger",
  info: "info",
};

export function useToast() {
  const { addToast: onceAddToast } = useOnceToast();

  const addToast = (message: string, variant: ToastVariant = "info") => {
    // Once UI only supports 'success' and 'danger' variants; map 'info' to 'success'
    const mapped = variantMap[variant];
    const safeVariant = mapped === "info" ? "success" : mapped;
    onceAddToast({
      variant: safeVariant,
      message,
    });
  };

  return { addToast };
}
