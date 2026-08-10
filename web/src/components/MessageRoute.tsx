import { useParams, useLocation, useNavigate, useSearchParams } from "react-router-dom";
import ListColumn from "./ListColumn";
import ConversationList from "../screens/ConversationList";
import MessageView from "../screens/MessageView";
import type { Conversation } from "../lib/types";

export default function MessageRoute() {
  const { conversationId } = useParams<{ conversationId: string }>();
  const location = useLocation();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();

  const conversationSearch = searchParams.get("q") || "";
  const conversationFilter = searchParams.get("f") || "";
  const query = conversationFilter || conversationSearch;

  const state = location.state as {
    conversation?: Conversation;
    openContactId?: string;
  } | null;
  const conversation = state?.conversation ?? null;
  const openContactId = state?.openContactId ?? null;

  const handleSearchChange = (q: string) => {
    const next = new URLSearchParams(searchParams);
    if (q) next.set("q", q); else next.delete("q");
    next.delete("f");
    setSearchParams(next, { replace: true });
  };

  const handleSearch = (q: string) => {
    navigate(`/messages/${conversationId}?q=${encodeURIComponent(q)}`, {
      state: { conversation, openContactId },
    });
  };

  return (
    <>
      <ListColumn
        searchQuery={conversationSearch}
        searchMode="messages"
        onSearchChange={handleSearchChange}
        onSearch={handleSearch}
      >
        <ConversationList
          selectedId={conversationId ?? null}
          onSelect={(c) =>
            navigate(`/messages/${c.id}`, {
              state: { conversation: c, openContactId },
            })
          }
          query={query}
        />
      </ListColumn>
      <main className="min-w-0 flex-1 overflow-auto bg-bg text-text">
        {conversation ? (
          <MessageView
            conversation={conversation}
            onOpenContact={(contactId: string) => {
              navigate(location.pathname + location.search, {
                state: { conversation, openContactId: contactId },
              });
            }}
          />
        ) : (
          <div className="flex h-full items-center justify-center text-[0.875rem] text-muted">
            Select a conversation to view messages
          </div>
        )}
      </main>
    </>
  );
}
