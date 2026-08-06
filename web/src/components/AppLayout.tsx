import { useState } from "react";
import LeftPanel from "./LeftPanel";
import type { ReactNode } from "react";

export default function AppLayout({ children }: { children: ReactNode }) {
  const [activeView, setActiveView] = useState("conversations");
  const [searchQuery, setSearchQuery] = useState("");

  return (
    <div style={{ display: "flex", height: "100vh", fontFamily: "system-ui" }}>
      <LeftPanel
        activeView={activeView}
        onNavigate={setActiveView}
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        onSearch={(q) => {
          setSearchQuery(q);
          setActiveView("conversations");
        }}
      />
      <main style={{ flex: 1, overflow: "auto", background: "#fff" }}>
        {children}
      </main>
    </div>
  );
}
