import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const badgeVariants = cva(
  "inline-flex items-center rounded-full border px-2.5 py-0.5 text-xs font-semibold transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2",
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
          "border-transparent bg-success/12 text-success",
        error:
          "border-transparent bg-error/12 text-error",
        running:
          "border-transparent bg-running/12 text-running",
        warning:
          "border-transparent bg-warning/12 text-warning",
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

export function statusToBadgeVariant(
  status: string,
  exitCode?: number | null
): "success" | "error" | "running" | "warning" | "disabled" | "default" {
  switch (status) {
    case "Completed":
      return exitCode === 0 ? "success" : "error";
    case "Failed":
      return "error";
    case "Running":
      return "running";
    case "Killed":
      return "warning";
    case "CompletedWithWarnings":
      return "warning";
    default:
      return "default";
  }
}

export { Badge, badgeVariants }
