"use client";

import dynamic from "next/dynamic";

const AppShell = dynamic(
  () => import("@/components/layout/AppShell").then((mod) => ({ default: mod.AppShell })),
  { ssr: false }
);

export default function Page() {
  return <AppShell />;
}
