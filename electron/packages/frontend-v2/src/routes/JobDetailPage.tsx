import { useParams } from "react-router-dom";

export function JobDetailPage() {
  const { id } = useParams<{ id: string }>();
  return (
    <div className="p-6">
      <h1 className="text-xl font-semibold">Job Detail</h1>
      <p className="text-sm text-muted-foreground mt-2">Job ID: {id} — coming in #109</p>
    </div>
  );
}
