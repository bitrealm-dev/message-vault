import { useState } from "react";
import LeftPanel from "./LeftPanel";
import ConversationList from "../screens/ConversationList";
import ContactList from "../screens/ContactList";
import ContactDrawer from "./ContactDrawer";
import ImportScreen from "../screens/ImportScreen";
import ExportScreen from "../screens/ExportScreen";
import TrashScreen from "../screens/TrashScreen";
import SettingsScreen from "../screens/SettingsScreen";
import ImportHistoryScreen from "../screens/ImportHistoryScreen";
import MessageView from "../screens/MessageView";
import SearchResults from "../screens/SearchResults";
import type { Conversation } from "../lib/types";

export default function AppLayout() {
  const [activeView, setActiveView] = useState("conversations");
  const [selectedConversation, setSelectedConversation] = useState<Conversation | null>(null);
  const [selectedContactId, setSelectedContactId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchActive, setSearchActive] = useState(false);
  const [findTerm, setFindTerm] = useState("");
  const [exportScope] = useState<"all" | "current-view" | "selected">("all");

  const handleSearch = (q: string) => {
    setSearchQuery(q);
    setActiveView("conversations");
    setSearchActive(q.trim() !== "");
  };

  const handleSearchChange = (q: string) => {
    setSearchQuery(q);
    if (!q.trim()) setSearchActive(false);
  };

  const handleSelectResult = (conversation: Conversation, term: string) => {
    setSelectedConversation(conversation);
    setActiveView("conversations");
    setFindTerm(term);
  };

  const leftContent =
    activeView === "conversations" || activeView === "trash" ? (
      searchActive && searchQuery.trim() && activeView === "conversations" ? (
        <SearchResults query={searchQuery} onSelectResult={handleSelectResult} />
      ) : (
        <ConversationList
          selectedId={selectedConversation?.id || null}
          onSelect={(c) => { setSelectedConversation(c); setActiveView("conversations"); }}
          query={activeView === "trash" ? "is:trash" : searchQuery}
        />
      )
    ) : activeView === "contacts" ? (
      <ContactList onSelect={(c) => setSelectedContactId(c.id)} />
    ) : null;

  const mainContent = () => {
    switch (activeView) {
      case "conversations":
        return selectedConversation ? (
          <MessageView
            conversation={selectedConversation}
            onOpenContact={(contactId: string) => setSelectedContactId(contactId)}
            initialFindTerm={findTerm}
          />
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
      case "import-history": return <ImportHistoryScreen />;
      case "export": return <ExportScreen scope={exportScope} selectedCount={0} />;
      case "settings":
      case "profile":
        return <SettingsScreen />;
      default: return null;
    }
  };

  return (
    <div style={{ display: "flex", height: "100vh", fontFamily: "system-ui" }}>
      <LeftPanel
        activeView={activeView}
        onNavigate={setActiveView}
        searchQuery={searchQuery}
        onSearchChange={handleSearchChange}
        onSearch={handleSearch}
        conversationList={leftContent}
      />
      <main style={{ flex: 1, overflow: "auto", background: "#fff" }}>
        {mainContent()}
      </main>
      <ContactDrawer contactId={selectedContactId} onClose={() => setSelectedContactId(null)} />
    </div>
  );
}
