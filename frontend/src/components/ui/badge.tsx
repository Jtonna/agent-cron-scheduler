import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const badgeVariants = cva(
  "inline-flex items-center rounded-md border px-2.5 py-0.5 text-xs font-semibold transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2",
  {
    variants: {
      variant: {
        default:
          "border-transparent bg-primary text-primary-foreground shadow hover:bg-primary/80",
        secondary:
          "border-transparent bg-secondary text-secondary-foreground hover:bg-secondary/80",
        destructive:
          "border-transparent bg-destructive text-destructive-foreground shadow hover:bg-destructive/80",
        outline: "text-foreground",
        success:
          "border-transparent bg-success/15 text-success-foreground",
        error:
          "border-transparent bg-destructive/15 text-destructive",
        warning:
          "border-transparent bg-warning/15 text-warning-foreground",
        running:
          "border-transparent bg-primary/15 text-primary",
        disabled:
          "border-transparent bg-muted text-muted-foreground",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
)

export interface BadgeProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof badgeVariants> {}

function Badge({ className, variant, ...props }: BadgeProps) {
  return (
    <div className={cn(badgeVariants({ variant }), className)} {...props} />
  )
}

export { Badge, badgeVariants }

export type BadgeVariant = NonNullable<VariantProps<typeof badgeVariants>["variant"]>;

export function statusToBadgeVariant(
  status: string,
  exitCodeOrEnabled?: number | null | boolean
): BadgeVariant {
  // Legacy: if a boolean is passed, treat as "enabled" flag
  if (exitCodeOrEnabled === false) return "disabled";
  switch (status) {
    case "Completed":
      // Non-zero exit code = error even if status is "Completed"
      if (typeof exitCodeOrEnabled === "number" && exitCodeOrEnabled !== 0) return "error";
      return "success";
    case "Failed":
      return "error";
    case "Running":
      return "running";
    case "Killed":
      return "warning";
    default:
      return "default";
  }
}
