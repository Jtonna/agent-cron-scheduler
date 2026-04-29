import Link from "next/link";
import { Navbar } from "@/components/Navbar";
import { ArrowLeft, Ghost } from "lucide-react";

export default function NotFound() {
  return (
    <div className="min-h-screen bg-surface text-fg flex flex-col">
      <Navbar />
      <main className="flex-1 flex flex-col items-center justify-center px-8">
        <Ghost size={48} className="text-fg-faint mb-6" strokeWidth={1.5} />
        <h1 className="text-[72px] font-extrabold tracking-tight leading-none text-fg">404</h1>
        <p className="text-fg-muted text-[15px] mt-3 mb-8">This page doesn&apos;t exist or has been moved.</p>
        <Link
          href="/"
          className="inline-flex items-center gap-2 bg-brand hover:bg-brand-hover text-surface text-sm font-semibold px-5 py-2.5 rounded-pill transition-colors"
        >
          <ArrowLeft size={14} />
          Back to dashboard
        </Link>
      </main>
    </div>
  );
}
