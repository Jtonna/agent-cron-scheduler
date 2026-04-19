import { useState } from "react";
import { useParams } from "react-router-dom";
import { SubNavTabs } from "@/components/layout/SubNavTabs";

export function JobDetailPage() {
  const { id } = useParams<{ id: string }>();
  const [activeTab, setActiveTab] = useState("overview");

  return (
    <div>
      <SubNavTabs
        tabs={[
          { id: "overview", label: "Overview" },
          { id: "configuration", label: "Configuration" },
        ]}
        activeTab={activeTab}
        onTabChange={setActiveTab}
      />
      <div className="p-6">
        <h1 className="text-xl font-semibold">Job Detail — {activeTab}</h1>
        <p className="text-sm text-muted-foreground mt-2">Job ID: {id} — coming in #109</p>
      </div>
    </div>
  );
}
