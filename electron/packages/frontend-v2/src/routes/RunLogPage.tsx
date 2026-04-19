import { useParams } from "react-router-dom";

export function RunLogPage() {
  const { id, runId } = useParams<{ id: string; runId: string }>();
  return (
    <div className="p-6">
      <h1 className="text-xl font-semibold">Run Log</h1>
      <p className="text-sm text-muted-foreground mt-2">Job: {id} / Run: {runId} — coming in #109</p>
    </div>
  );
}
