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
          "border-transparent bg-green-500/15 text-green-700 dark:text-green-400",
        error:
          "border-transparent bg-red-500/15 text-red-700 dark:text-red-400",
        running:
          "border-transparent bg-blue-500/15 text-blue-700 dark:text-blue-400",
        warning:
          "border-transparent bg-yellow-500/15 text-yellow-700 dark:text-yellow-400",
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
