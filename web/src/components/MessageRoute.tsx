import { useState, useEffect } from "react";
import { useParams, useLocation, useNavigate, useSearchParams, Link } from "react-router-dom";
import ListColumn from "./ListColumn";
import ConversationList from "../screens/ConversationList";
import MessageView from "../screens/MessageView";
import type { Conversation } from "../lib/types";
import { asMessagesLocationState } from "../lib/messagesLocationState";
import { fetchConversationById } from "../lib/fetchConversationById";

export default function MessageRoute() {
  const { conversationId } = useParams<{ conversationId: string }>();
  const location = useLocation();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();

  const conversationSearch = searchParams.get("q") || "";
  const conversationFilter = searchParams.get("f") || "";
  const query = conversationFilter || conversationSearch;

  const locationState = asMessagesLocationState(location.state);
  const stateConversation = locationState?.conversation ?? null;
  const openContactId = locationState?.openContactId ?? null;

  const [fetchedConversation, setFetchedConversation] = useState<Conversation | null>(null);
  const [fetchLoading, setFetchLoading] = useState(false);
  const [fetchError, setFetchError] = useState<string | null>(null);

  const conversation = stateConversation ?? fetchedConversation;

  useEffect(() => {
    if (stateConversation || !conversationId) {
      setFetchedConversation(null);
      setFetchLoading(false);
      setFetchError(null);
      return;
    }

    const controller = new AbortController();
    setFetchLoading(true);
    setFetchError(null);
    setFetchedConversation(null);

    void (async () => {
      try {
        const found = await fetchConversationById(conversationId, controller.signal);
        if (controller.signal.aborted) return;
        if (found) {
          setFetchedConversation(found);
        } else {
          setFetchError("Conversation not found.");
        }
      } catch (e) {
        if (controller.signal.aborted) return;
        setFetchError(String(e));
      } finally {
        if (!controller.signal.aborted) {
          setFetchLoading(false);
        }
      }
    })();

    return () => controller.abort();
  }, [conversationId, stateConversation]);

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
        ) : fetchLoading ? (
          <div className="flex h-full items-center justify-center text-[0.875rem] text-muted">
            Loading conversation…
          </div>
        ) : fetchError ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
            <p className="m-0 text-[0.875rem] text-danger">{fetchError}</p>
            <Link
              to="/"
              className="text-[0.875rem] text-accent underline-offset-2 hover:underline"
            >
              Back to conversations
            </Link>
          </div>
        ) : (
          <div className="flex h-full items-center justify-center text-[0.875rem] text-muted">
            Select a conversation to view messages
          </div>
        )}
      </main>
    </>
  );
}
