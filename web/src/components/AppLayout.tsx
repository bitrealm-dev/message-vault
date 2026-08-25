import { useCallback, useState } from "react";
import { Outlet, useLocation, useNavigate, useSearchParams } from "react-router-dom";
import { groupFromSlug } from "../lib/contactGroups";
import { asMessagesLocationState } from "../lib/messagesLocationState";
import { tagFromSlug, tagListQuery } from "../lib/threadTags";
import type { Conversation } from "../lib/types";
import { useContactGroups } from "../lib/useContactGroups";
import { useThreadTags } from "../lib/useThreadTags";
import ContactList from "../screens/ContactList";
import ConversationList from "../screens/ConversationList";
import AppHeader from "./AppHeader";
import CheckedContactsPanel from "./CheckedContactsPanel";
import { ColumnResizeProvider } from "./ColumnResizeContext";
import ContactDrawer from "./ContactDrawer";
import {
  type ContactBrowseKind,
  type ContactListPreviewSource,
  type ContactPreview,
  contactPreviewFromListRow,
} from "./contactDrawer/contactDrawerTypes";
import LeftPanel from "./LeftPanel";
import ListColumn from "./ListColumn";
import RightPane from "./RightPane";
import { RightToolbarProvider } from "./RightToolbarContext";

/** Search query used when browsing a contact's conversations from the drawer. */
function contactBrowseQuery(
  contactId: string,
  kind: ContactBrowseKind,
  handle?: string,
  service?: string,
): string {
  let typeSuffix = "";
  if (kind === "direct") typeSuffix = " is:direct";
  else if (kind === "group") typeSuffix = " is:group";

  const h = handle?.trim();
  if (h) {
    const quoted = /\s/.test(h) ? `"${h}"` : h;
    const platform = service?.trim().toLowerCase();
    const serviceSuffix =
      platform === "phone" || platform === "whatsapp" ? ` service:${platform}` : "";
    return `handle:${quoted}${serviceSuffix}${typeSuffix}`;
  }
  return `contact:${contactId}${typeSuffix}`;
}

type ColumnMode = "conversations" | "contacts" | "trash" | "import" | "export" | "settings";

/** Which left-column list to show for this URL. */
function modeFromPathname(pathname: string): ColumnMode {
  if (pathname.startsWith("/messages/")) return "conversations";
  if (pathname === "/contacts" || pathname === "/no-group" || pathname.startsWith("/group/")) {
    return "contacts";
  }
  if (pathname === "/no-tag" || pathname.startsWith("/tag/")) {
    return "conversations";
  }
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
  const [checkedContacts, setCheckedContacts] = useState<ContactPreview[]>([]);
  const [clearCheckedRev, setClearCheckedRev] = useState(0);
  const handleCheckedContacts = useCallback((contacts: ContactListPreviewSource[]) => {
    setCheckedContacts(contacts.map(contactPreviewFromListRow));
  }, []);
  const clearCheckedContacts = useCallback(() => {
    setClearCheckedRev((n) => n + 1);
  }, []);
  const { groups } = useContactGroups();
  const { tags } = useThreadTags();

  const pathname = location.pathname;
  const mode = modeFromPathname(pathname);
  const isMessageRoute = pathname.startsWith("/messages/");
  const contactsMode = mode === "contacts";
  const noGroupMode = pathname === "/no-group";
  const groupSlugParam = pathname.startsWith("/group/")
    ? decodeURIComponent(pathname.slice("/group/".length))
    : null;
  const activeGroup = groupSlugParam ? groupFromSlug(groupSlugParam, groups) : null;
  const groupFilter = noGroupMode ? "none" : activeGroup;
  const noTagMode = pathname === "/no-tag";
  const tagSlugParam = pathname.startsWith("/tag/")
    ? decodeURIComponent(pathname.slice("/tag/".length))
    : null;
  const activeTag = tagSlugParam ? tagFromSlug(tagSlugParam, tags) : null;
  const tagFilter = noTagMode ? "none" : activeTag;

  const conversationSearch = searchParams.get("q") || "";
  const conversationFilter = searchParams.get("f") || "";
  const contactSearch = searchParams.get("cq") || "";

  const searchQuery = contactsMode ? contactSearch : conversationSearch;

  function updateSearchParams(updates: Record<string, string>) {
    const next = new URLSearchParams(searchParams);
    for (const [k, v] of Object.entries(updates)) {
      if (v) next.set(k, v);
      else next.delete(k);
    }
    setSearchParams(next, { replace: true });
  }

  const handleSearch = (q: string) => {
    if (/\bsearch:contacts\b/i.test(q) || contactsMode) {
      const params = q ? `?cq=${encodeURIComponent(q)}` : "";
      if (noGroupMode) {
        navigate(`/no-group${params}`);
      } else if (groupSlugParam) {
        navigate(`/group/${groupSlugParam}${params}`);
      } else {
        navigate(`/contacts${params}`);
      }
    } else if (pathname.startsWith("/messages/")) {
      const id = pathname.split("/")[2];
      if (id) {
        navigate(`/messages/${id}?q=${encodeURIComponent(q)}`, {
          state: location.state,
        });
        return;
      }
      navigate(`/?q=${encodeURIComponent(q)}`);
    } else if (noTagMode) {
      navigate(`/no-tag${q ? `?q=${encodeURIComponent(q)}` : ""}`);
    } else if (tagSlugParam) {
      navigate(`/tag/${tagSlugParam}${q ? `?q=${encodeURIComponent(q)}` : ""}`);
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

  const threadListQuery = tagListQuery(tagFilter, conversationFilter || conversationSearch);

  const handleConversationSelect = (c: Conversation) => {
    const params = tagFilter ? `?q=${encodeURIComponent(threadListQuery)}` : "";
    navigate(`/messages/${c.id}${params}`, { state: { conversation: c } });
  };

  const closeContactDrawer = () => {
    setSelectedContact(null);
    const state = asMessagesLocationState(location.state);
    if (!state?.openContactId) return;
    const { openContactId: _closed, ...rest } = state;
    navigate(`${location.pathname}${location.search}`, {
      replace: true,
      state: Object.keys(rest).length > 0 ? rest : null,
    });
  };

  const handleBrowseContactConversations = ({
    contactId,
    kind,
    handle,
    service,
  }: {
    contactId: string;
    kind: ContactBrowseKind;
    handle?: string;
    service?: string;
    handles?: string[];
  }) => {
    const query = contactBrowseQuery(contactId, kind, handle, service);
    setSelectedContact(null);
    navigate(`/?q=${encodeURIComponent(query)}&f=${encodeURIComponent(query)}`);
  };

  const isFullScreen = mode === "import" || mode === "export" || mode === "settings";
  const isTrash = mode === "trash";

  // Contact drawer: MessageRoute stores the contact id on location state.
  const openContactId = asMessagesLocationState(location.state)?.openContactId ?? null;

  return (
    <RightToolbarProvider>
      <div className="flex h-screen flex-col bg-bg font-sans text-text">
        <AppHeader
          searchQuery={searchQuery}
          searchMode={contactsMode ? "contacts" : "messages"}
          onSearchChange={handleSearchChange}
          onSearch={handleSearch}
        />
        <ColumnResizeProvider>
          <div className="flex min-h-0 flex-1 overflow-hidden">
            <LeftPanel onSearchChange={handleSearchChange} onSearch={handleSearch} />

            {/* Conversations: render list component directly with props */}
            {mode === "conversations" && !isMessageRoute && (
              <>
                <ListColumn>
                  <ConversationList
                    selectedId={null}
                    onSelect={handleConversationSelect}
                    query={threadListQuery}
                  />
                </ListColumn>
                <RightPane>
                  <main className={mainPane}>
                    <div className={emptyMain}>Select a conversation to view messages</div>
                  </main>
                </RightPane>
              </>
            )}

            {/* Contacts: render list component directly with props */}
            {mode === "contacts" && (
              <>
                <ListColumn>
                  <ContactList
                    filter={contactSearch}
                    groupFilter={groupFilter}
                    selectedId={selectedContact?.id ?? null}
                    onSelect={(c) => setSelectedContact(contactPreviewFromListRow(c))}
                    onCheckedChange={handleCheckedContacts}
                    clearCheckedRev={clearCheckedRev}
                  />
                </ListColumn>
                <RightPane>
                  {checkedContacts.length > 0 ? (
                    <CheckedContactsPanel
                      contacts={checkedContacts}
                      onClear={clearCheckedContacts}
                    />
                  ) : selectedContact ? (
                    <ContactDrawer
                      variant="docked"
                      contactId={selectedContact.id}
                      preview={selectedContact}
                      onClose={closeContactDrawer}
                      onBrowseConversations={handleBrowseContactConversations}
                    />
                  ) : (
                    <main className={mainPane}>
                      <div className={emptyMain}>Select a contact to view details</div>
                    </main>
                  )}
                </RightPane>
              </>
            )}

            {/* Trash: ListColumn shows ConversationList with trash query; main shows TrashScreen via <Outlet /> */}
            {isTrash && (
              <>
                <ListColumn>
                  <ConversationList
                    selectedId={null}
                    // Trash thread selection is handled by TrashScreen in the outlet.
                    onSelect={() => {}}
                    query="is:trash"
                  />
                </ListColumn>
                <RightPane>
                  <main className={mainPane}>
                    <Outlet />
                  </main>
                </RightPane>
              </>
            )}

            {/* Message route: single <Outlet /> — MessageRoute renders both ListColumn + main */}
            {isMessageRoute && (
              <div className="flex min-w-0 flex-1 overflow-hidden">
                <Outlet />
              </div>
            )}

            {/* Full-screen views: no ListColumn, just main */}
            {isFullScreen && (
              <main className={mainPane}>
                <Outlet />
              </main>
            )}

            {/* Overlay contact panel (e.g. opened from a message thread). */}
            {openContactId ? (
              <ContactDrawer
                variant="overlay"
                contactId={openContactId}
                onClose={closeContactDrawer}
                onBrowseConversations={handleBrowseContactConversations}
              />
            ) : null}
          </div>
        </ColumnResizeProvider>
      </div>
    </RightToolbarProvider>
  );
}
