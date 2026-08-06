import { useState } from "react";
import LeftPanel from "./LeftPanel";
import ConversationList from "../screens/ConversationList";
import MessageView from "../screens/MessageView";
import type { Conversation } from "../lib/types";

export default function AppLayout() {
  const [activeView, setActiveView] = useState("conversations");
  const [selectedConversation, setSelectedConversation] = useState<Conversation | null>(null);
  const [searchQuery, setSearchQuery] = useState("");

  const leftContent =
    activeView === "conversations" || activeView === "trash" ? (
      <ConversationList
        selectedId={selectedConversation?.id || null}
        onSelect={(c) => {
          setSelectedConversation(c);
          setActiveView("conversations");
        }}
        query={activeView === "trash" ? "is:trash" : searchQuery}
      />
    ) : null;

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
        conversationList={leftContent}
      />
      <main style={{ flex: 1, overflow: "auto", background: "#fff" }}>
        {selectedConversation && activeView === "conversations" ? (
          <MessageView conversation={selectedConversation} />
        ) : (
          <div style={{
            display: "flex", alignItems: "center", justifyContent: "center",
            height: "100%", color: "#9ca3af", fontSize: "0.875rem",
          }}>
            Select a conversation to view messages
          </div>
        )}
      </main>
    </div>
  );
}
