import { useState } from "react";
import { Outlet, useLocation, useNavigate, useSearchParams } from "react-router-dom";
import LeftPanel from "./LeftPanel";
import ListColumn from "./ListColumn";
import ConversationList from "../screens/ConversationList";
import ContactDrawer, {
  type ContactBrowseKind,
  type ContactPreview,
} from "./ContactDrawer";
import ContactList from "../screens/ContactList";
import type { Conversation } from "../lib/types";

function contactBrowseQuery(contactId: string, kind: ContactBrowseKind): string {
  if (kind === "direct") return `contact:${contactId} is:direct`;
  if (kind === "group") return `contact:${contactId} is:group`;
  return `contact:${contactId}`;
}

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

type ColumnMode = "conversations" | "contacts" | "trash" | "import" | "export" | "settings";

function modeFromPathname(pathname: string): ColumnMode {
  if (pathname.startsWith("/messages/")) return "conversations";
  if (pathname === "/contacts") return "contacts";
  if (pathname === "/trash") return "trash";
  if (pathname === "/import") return "import";
  if (pathname === "/export") return "export";
  if (pathname === "/settings") return "settings";
  return "conversations";
}

/** Scrollable content column that hosts the routed screen. */
const mainPane = "min-w-0 flex-1 overflow-auto bg-bg text-text";

/** Centered placeholder when a column has nothing selected yet. */
const emptyMain = "flex h-full items-center justify-center text-[0.875rem] text-muted";

export default function AppLayout() {
  const location = useLocation();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();

  const [selectedContact, setSelectedContact] = useState<ContactPreview | null>(null);

  const pathname = location.pathname;
  const mode = modeFromPathname(pathname);
  const isMessageRoute = pathname.startsWith("/messages/");
  const contactsMode = mode === "contacts";

  const conversationSearch = searchParams.get("q") || "";
  const conversationFilter = searchParams.get("f") || "";
  const contactSearch = searchParams.get("cq") || "";

  const searchQuery = contactsMode ? contactSearch : conversationSearch;

  function updateSearchParams(updates: Record<string, string>) {
    const next = new URLSearchParams(searchParams);
    for (const [k, v] of Object.entries(updates)) {
      if (v) next.set(k, v); else next.delete(k);
    }
    setSearchParams(next, { replace: true });
  }

  const handleSearch = (q: string) => {
    if (/\bsearch:contacts\b/i.test(q) || contactsMode) {
      navigate(`/contacts?cq=${encodeURIComponent(q)}`);
    } else {
      navigate(`/?q=${encodeURIComponent(q)}`);
    }
  };

  const handleSearchChange = (q: string) => {
    if (contactsMode) {
      updateSearchParams({ cq: q });
      return;
    }
    updateSearchParams({ q: q, f: "" });
  };

  const handleConversationSelect = (c: Conversation) => {
    navigate(`/messages/${c.id}`, { state: { conversation: c } });
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
    navigate(`/?q=${encodeURIComponent(visible)}&f=${encodeURIComponent(apiQuery)}`);
  };

  const isFullScreen = mode === "import" || mode === "export" || mode === "settings";
  const isTrash = mode === "trash";

  // Contact drawer: read openContactId from location state (set by MessageRoute)
  const locationState = location.state as { openContactId?: string } | null;
  const openContactId = locationState?.openContactId ?? null;

  return (
    <div className="flex h-screen bg-bg font-sans text-text">
      <LeftPanel
        onSearchChange={handleSearchChange}
        onSearch={handleSearch}
      />

      {/* Conversations: render list component directly with props */}
      {mode === "conversations" && !isMessageRoute && (
        <>
          <ListColumn
            searchQuery={searchQuery}
            searchMode="messages"
            onSearchChange={handleSearchChange}
            onSearch={handleSearch}
          >
            <ConversationList
              selectedId={null}
              onSelect={handleConversationSelect}
              query={conversationFilter || conversationSearch}
            />
          </ListColumn>
          <main className={mainPane}>
            <div className={emptyMain}>Select a conversation to view messages</div>
          </main>
        </>
      )}

      {/* Contacts: render list component directly with props */}
      {mode === "contacts" && (
        <>
          <ListColumn
            searchQuery={contactSearch}
            searchMode="contacts"
            onSearchChange={handleSearchChange}
            onSearch={handleSearch}
          >
            <ContactList
              filter={contactSearch}
              onSelect={(c) =>
                setSelectedContact({ id: c.id, name: c.name, handles: c.handles })
              }
            />
          </ListColumn>
          <main className={mainPane}>
            <div className={emptyMain}>Select a contact to view details</div>
          </main>
        </>
      )}

      {/* Trash: ListColumn shows ConversationList with trash query; main shows TrashScreen via <Outlet /> */}
      {isTrash && (
        <>
          <ListColumn
            searchQuery=""
            searchMode="messages"
            onSearchChange={handleSearchChange}
            onSearch={handleSearch}
          >
            <ConversationList
              selectedId={null}
              onSelect={() => {}}
              query="is:trash"
            />
          </ListColumn>
          <main className={mainPane}>
            <Outlet />
          </main>
        </>
      )}

      {/* Message route: single <Outlet /> — MessageRoute renders both ListColumn + main */}
      {isMessageRoute && (
        <div className="flex min-w-0 flex-1">
          <Outlet />
        </div>
      )}

      {/* Full-screen views: no ListColumn, just main */}
      {isFullScreen && (
        <main className={mainPane}>
          <Outlet />
        </main>
      )}

      <ContactDrawer
        contactId={selectedContact?.id ?? openContactId ?? null}
        preview={selectedContact}
        onClose={() => setSelectedContact(null)}
        onBrowseConversations={handleBrowseContactConversations}
      />
    </div>
  );
}
