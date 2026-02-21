import { EditJobContent } from "./EditJobContent";

export async function generateStaticParams() {
  // Return a placeholder so Next.js static export accepts the dynamic route.
  // Actual routing is handled client-side via the SPA fallback.
  return [{ id: "_" }];
}

export default function EditJobPage() {
  return <EditJobContent />;
}
