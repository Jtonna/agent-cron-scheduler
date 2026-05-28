"use client";

import { use, useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { Loader2 } from "lucide-react";
import { Navbar } from "@/components/navbar/Navbar";
import { Button } from "@/components/ui/Button";
import { useJob } from "@/apis/useJob";
import { useUpdateWorkflow } from "@/apis/useUpdateWorkflow";

/**
 * Edit workflow page (`/jobs/[id]/edit`).
 *
 * Renders the workflow JSON in a tall, full-width textarea so power users
 * can directly edit the definition. Validates JSON client-side on Save;
 * if the parse succeeds, PATCHes via `useUpdateWorkflow`. Parse errors and
 * server 4xx/5xx errors surface in an inline banner. On success the user
 * is sent back to the workflow detail page.
 */
export default function EditWorkflowPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const router = useRouter();

  const { job, loading, error: loadError } = useJob(id);
  const { update, updating, error: serverError } = useUpdateWorkflow(id);

  const [text, setText] = useState<string>("");
  const [parseError, setParseError] = useState<string | null>(null);
  const [initialized, setInitialized] = useState(false);

  // Seed the textarea once the job arrives. Without the `initialized` guard
  // any cache refetch (e.g. an SSE invalidation) would clobber the user's
  // in-flight edits.
  useEffect(() => {
    if (!initialized && job) {
      setText(JSON.stringify(job, null, 2));
      setInitialized(true);
    }
  }, [job, initialized]);

  const goBack = () => router.push(`/jobs/${id}`);

  const handleSave = async () => {
    setParseError(null);
    let parsed: unknown;
    try {
      parsed = JSON.parse(text);
    } catch (err) {
      setParseError(err instanceof Error ? err.message : "Invalid JSON");
      return;
    }
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      setParseError("Workflow definition must be a JSON object.");
      return;
    }
    try {
      await update(parsed as Record<string, unknown>);
      router.push(`/jobs/${id}`);
    } catch {
      // serverError from the hook will surface the message below.
    }
  };

  const bannerMessage = parseError ?? serverError ?? null;

  return (
    <div className="min-h-screen bg-surface text-fg">
      <Navbar />

      <main className="mx-auto max-w-5xl px-8 py-10 flex flex-col gap-6">
        {loading && !job ? (
          <div className="flex items-center justify-center py-24">
            <Loader2 size={24} className="animate-spin text-fg-subtle" />
          </div>
        ) : loadError ? (
          <div className="rounded-card border border-status-failed-border bg-status-failed-bg p-4 text-status-failed text-sm">
            Failed to load workflow: {loadError}
          </div>
        ) : !job ? (
          <div className="text-fg-muted text-sm">
            Workflow <span className="font-mono">{id}</span> doesn&apos;t exist or has been deleted.
          </div>
        ) : (
          <>
            <header className="flex flex-col gap-1">
              <h1 className="text-2xl font-semibold text-fg">Edit workflow</h1>
              <p className="text-sm text-fg-muted">{job.name}</p>
            </header>

            {bannerMessage ? (
              <div
                role="alert"
                className="rounded-card border border-status-failed-border bg-status-failed-bg p-4 text-status-failed text-sm whitespace-pre-wrap"
              >
                {bannerMessage}
              </div>
            ) : null}

            <textarea
              value={text}
              onChange={(e) => setText(e.target.value)}
              spellCheck={false}
              className="w-full min-h-[600px] rounded-input border border-border bg-surface-secondary p-4 font-mono text-xs text-fg outline-none focus:border-border-active focus:ring-2 focus:ring-brand-ring resize-y"
            />

            <footer className="flex items-center justify-end gap-3">
              <Button intent="secondary" onPress={goBack} isDisabled={updating}>
                Cancel
              </Button>
              <Button intent="primary" onPress={handleSave} isDisabled={updating}>
                {updating ? "Saving…" : "Save"}
              </Button>
            </footer>
          </>
        )}
      </main>
    </div>
  );
}
