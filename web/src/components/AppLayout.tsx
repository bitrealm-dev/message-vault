import { useState } from "react";
import LeftPanel from "./LeftPanel";
import ConversationList from "../screens/ConversationList";
import ContactList from "../screens/ContactList";
import ContactDrawer from "./ContactDrawer";
import ImportScreen from "../screens/ImportScreen";
import ExportScreen from "../screens/ExportScreen";
import TrashScreen from "../screens/TrashScreen";
import SettingsScreen from "../screens/SettingsScreen";
import ProfileScreen from "../screens/ProfileScreen";
import MessageView from "../screens/MessageView";
import type { Conversation } from "../lib/types";

export default function AppLayout() {
  const [activeView, setActiveView] = useState("conversations");
  const [selectedConversation, setSelectedConversation] = useState<Conversation | null>(null);
  const [selectedContactId, setSelectedContactId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [exportScope] = useState<"all" | "current-view" | "selected">("all");

  const leftContent =
    activeView === "conversations" || activeView === "trash" ? (
      <ConversationList
        selectedId={selectedConversation?.id || null}
        onSelect={(c) => { setSelectedConversation(c); setActiveView("conversations"); }}
        query={activeView === "trash" ? "is:trash" : searchQuery}
      />
    ) : activeView === "contacts" ? (
      <ContactList onSelect={(c) => setSelectedContactId(c.id)} />
    ) : null;

  const mainContent = () => {
    switch (activeView) {
      case "conversations":
        return selectedConversation ? (
          <MessageView conversation={selectedConversation} />
        ) : (
          <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100%", color: "#9ca3af", fontSize: "0.875rem" }}>
            Select a conversation to view messages
          </div>
        );
      case "contacts":
        return (
          <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100%", color: "#9ca3af", fontSize: "0.875rem" }}>
            Select a contact to view details
          </div>
        );
      case "trash": return <TrashScreen />;
      case "import": return <ImportScreen />;
      case "export": return <ExportScreen scope={exportScope} selectedCount={0} />;
      case "settings": return <SettingsScreen />;
      case "profile": return <ProfileScreen />;
      default: return null;
    }
  };

  return (
    <div style={{ display: "flex", height: "100vh", fontFamily: "system-ui" }}>
      <LeftPanel
        activeView={activeView}
        onNavigate={setActiveView}
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        onSearch={(q) => { setSearchQuery(q); setActiveView("conversations"); }}
        conversationList={leftContent}
      />
      <main style={{ flex: 1, overflow: "auto", background: "#fff" }}>
        {mainContent()}
      </main>
      <ContactDrawer contactId={selectedContactId} onClose={() => setSelectedContactId(null)} />
    </div>
  );
}
