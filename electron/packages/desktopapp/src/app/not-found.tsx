import Link from "next/link";
import { Navbar } from "@/components/Navbar";
import { ArrowLeft, Ghost } from "lucide-react";

export default function NotFound() {
  return (
    <div className="min-h-screen bg-white text-gray-900 flex flex-col">
      <Navbar />
      <main className="flex-1 flex flex-col items-center justify-center px-8">
        <Ghost size={48} className="text-gray-300 mb-6" strokeWidth={1.5} />
        <h1 className="text-[72px] font-extrabold tracking-tight leading-none text-gray-900">404</h1>
        <p className="text-gray-500 text-[15px] mt-3 mb-8">This page doesn&apos;t exist or has been moved.</p>
        <Link
          href="/"
          className="inline-flex items-center gap-2 bg-pink-500 hover:bg-pink-600 text-white text-sm font-semibold px-5 py-2.5 rounded-full transition-colors"
        >
          <ArrowLeft size={14} />
          Back to dashboard
        </Link>
      </main>
    </div>
  );
}
