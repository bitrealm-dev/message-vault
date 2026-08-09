import { useState, type ReactNode } from "react";
import LeftPanel from "./LeftPanel";
import ListColumn from "./ListColumn";
import ConversationList, {
  type ConversationAutoSelect,
} from "../screens/ConversationList";
import ContactList from "../screens/ContactList";
import ContactDrawer, {
  type ContactBrowseKind,
  type ContactPreview,
} from "./ContactDrawer";
import ImportScreen from "../screens/ImportScreen";
import ExportScreen from "../screens/ExportScreen";
import TrashScreen from "../screens/TrashScreen";
import SettingsScreen from "../screens/SettingsScreen";
import MessageView from "../screens/MessageView";
import type { Conversation } from "../lib/types";
import type { ActiveView } from "../lib/views";

function contactBrowseQuery(contactId: string, kind: ContactBrowseKind): string {
  if (kind === "direct") return `contact:${contactId} is:direct`;
  if (kind === "group") return `contact:${contactId} is:group`;
  return `contact:${contactId}`;
}

/** Human-readable search-box text (handle + optional type), not contact ids. */
function visibleBrowseQuery(
  kind: ContactBrowseKind,
  handles: string[],
  contactId: string,
): string {
  const typeSuffix =
    kind === "direct" ? " is:direct" : kind === "group" ? " is:group" : "";
  const handle = handles.find((h) => h.trim().length > 0)?.trim();
  if (handle) return `handle:${handle}${typeSuffix}`;
  return `contact:${contactId}${typeSuffix}`;
}

const emptyMainStyle = {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  height: "100%",
  color: "var(--muted)",
  fontSize: "0.875rem",
} as const;

export default function AppLayout() {
  const [activeView, setActiveView] = useState<ActiveView>("conversations");
  const [selectedConversation, setSelectedConversation] = useState<Conversation | null>(null);
  const [selectedContact, setSelectedContact] = useState<ContactPreview | null>(null);
  /** Conversation list / message search box text. Independent of contact search. */
  const [conversationSearch, setConversationSearch] = useState("");
  /** Contact list search box text. Independent of conversation search. */
  const [contactSearch, setContactSearch] = useState("");
  /**
   * Hidden API filter for conversation list (e.g. `contact:4 is:direct`).
   * Kept separate so contact ids never appear in the search field.
   */
  const [conversationFilter, setConversationFilter] = useState("");
  const [autoSelect, setAutoSelect] = useState<ConversationAutoSelect | null>(null);

  const contactsMode = activeView === "contacts";
  const searchQuery = contactsMode ? contactSearch : conversationSearch;

  const handleSearch = (q: string) => {
    const isContactSearch =
      /\bsearch:contacts\b/i.test(q) || activeView === "contacts";
    if (isContactSearch) {
      setContactSearch(q);
      setActiveView("contacts");
    } else {
      setConversationFilter("");
      setConversationSearch(q);
      setActiveView("conversations");
      setAutoSelect(null);
    }
  };

  const handleSearchChange = (q: string) => {
    if (contactsMode) {
      setContactSearch(q);
      return;
    }
    setConversationFilter("");
    setConversationSearch(q);
    setAutoSelect(null);
  };

  const handleBrowseContactConversations = ({
    contactId,
    kind,
    handles = [],
  }: {
    contactId: string;
    kind: ContactBrowseKind;
    handles?: string[];
  }) => {
    const visible = visibleBrowseQuery(kind, handles, contactId);
    const apiQuery = contactBrowseQuery(contactId, kind);

    setSelectedContact(null);
    setActiveView("conversations");
    setConversationSearch(visible);
    setConversationFilter(apiQuery);
    setSelectedConversation(null);
    setAutoSelect(kind === "direct" ? "first" : "sole");
  };

  const conversationListQuery =
    activeView === "trash"
      ? "is:trash"
      : conversationFilter || conversationSearch;

  const showListColumn =
    activeView === "conversations" ||
    activeView === "contacts" ||
    activeView === "trash";

  const listContent =
    activeView === "conversations" || activeView === "trash" ? (
      <ConversationList
        selectedId={selectedConversation?.id || null}
        onSelect={(c) => {
          setSelectedConversation(c);
          setActiveView("conversations");
          setAutoSelect(null);
        }}
        query={conversationListQuery}
        autoSelect={autoSelect}
        onAutoSelectDone={() => setAutoSelect(null)}
      />
    ) : activeView === "contacts" ? (
      <ContactList
        filter={contactSearch}
        onSelect={(c) =>
          setSelectedContact({ id: c.id, name: c.name, handles: c.handles })
        }
      />
    ) : null;

  let mainContent: ReactNode;
  switch (activeView) {
    case "conversations":
      mainContent = selectedConversation ? (
        <MessageView
          conversation={selectedConversation}
          onOpenContact={(contactId: string) =>
            setSelectedContact({ id: contactId, name: "Loading…", handles: [] })
          }
        />
      ) : (
        <div style={emptyMainStyle}>Select a conversation to view messages</div>
      );
      break;
    case "contacts":
      mainContent = (
        <div style={emptyMainStyle}>Select a contact to view details</div>
      );
      break;
    case "trash":
      mainContent = <TrashScreen />;
      break;
    case "import":
      mainContent = <ImportScreen />;
      break;
    case "export":
      mainContent = <ExportScreen />;
      break;
    case "settings":
      mainContent = <SettingsScreen />;
      break;
  }

  return (
    <div style={{ display: "flex", height: "100vh", fontFamily: "system-ui", background: "var(--bg)", color: "var(--text)" }}>
      <LeftPanel
        activeView={activeView}
        onNavigate={setActiveView}
        onSearchChange={handleSearchChange}
        onSearch={handleSearch}
      />
      {showListColumn && (
        <ListColumn
          searchQuery={searchQuery}
          searchMode={contactsMode ? "contacts" : "messages"}
          onSearchChange={handleSearchChange}
          onSearch={handleSearch}
        >
          {listContent}
        </ListColumn>
      )}
      <main style={{ flex: 1, overflow: "auto", background: "var(--bg)", color: "var(--text)", minWidth: 0 }}>
        {mainContent}
      </main>
      <ContactDrawer
        contactId={selectedContact?.id ?? null}
        preview={selectedContact}
        onClose={() => setSelectedContact(null)}
        onBrowseConversations={handleBrowseContactConversations}
      />
    </div>
  );
}
