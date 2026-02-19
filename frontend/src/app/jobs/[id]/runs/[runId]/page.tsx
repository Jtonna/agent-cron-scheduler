import { RunLogContent } from "./RunLogContent";

export async function generateStaticParams() {
  return [{ id: "_", runId: "_" }];
}

export default function RunLogPage() {
  return <RunLogContent />;
}
