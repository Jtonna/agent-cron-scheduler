import { Navbar } from "@/components/navbar/Navbar";

export default function CreatePage() {
  return (
    <div className="min-h-screen bg-surface text-fg">
      <Navbar />
      <div className="px-16 py-14">
        <h1 className="text-3xl font-extrabold tracking-tight mb-2">Build a job</h1>
        <p className="text-fg-muted text-[15px]">Create a new scheduled or on-demand job.</p>
      </div>
    </div>
  );
}
