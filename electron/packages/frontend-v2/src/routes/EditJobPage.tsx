import { useParams } from "react-router-dom";

export function EditJobPage() {
  const { id } = useParams<{ id: string }>();
  return (
    <div className="p-6">
      <h1 className="text-xl font-semibold">Edit Job</h1>
      <p className="text-sm text-muted-foreground mt-2">Job ID: {id} — coming in #108</p>
    </div>
  );
}
