import { Badge } from "@/components/ui/badge";

interface JobStatusProps {
  enabled: boolean;
  last_exit_code: number | null;
  last_run_at: string | null;
}

export function JobStatusBadge({ enabled, last_exit_code, last_run_at }: JobStatusProps) {
  if (!enabled) return <Badge variant="disabled">Disabled</Badge>;
  if (last_exit_code === null && !last_run_at) return <Badge variant="default">Pending</Badge>;
  if (last_exit_code === 0) return <Badge variant="success">OK</Badge>;
  if (last_exit_code !== null) return <Badge variant="error">{`Failed (exit ${last_exit_code})`}</Badge>;
  return <Badge variant="default">Unknown</Badge>;
}
