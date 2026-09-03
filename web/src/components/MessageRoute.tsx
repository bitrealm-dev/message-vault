import { useEffect, useState } from "react";
import { Link, useLocation, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { fetchConversationById } from "../lib/fetchConversationById";
import { asMessagesLocationState } from "../lib/messagesLocationState";
import type { Conversation } from "../lib/types";
import ConversationList from "../screens/ConversationList";
import MessageView from "../screens/MessageView";
import ListColumn from "./ListColumn";
import RightPane from "./RightPane";

/** The route's `:conversationId` as a number, or null when it is not a positive integer. */
function positiveInteger(raw: string | undefined): number | null {
  if (raw === undefined || !/^\d+$/.test(raw)) return null;
  const n = Number(raw);
  return Number.isSafeInteger(n) && n > 0 ? n : null;
}

export default function MessageRoute() {
  const { conversationId: conversationParam } = useParams<{ conversationId: string }>();
  const conversationId = positiveInteger(conversationParam);
  const location = useLocation();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();

  const conversationSearch = searchParams.get("q") || "";
  const conversationFilter = searchParams.get("f") || "";
  const query = conversationFilter || conversationSearch;

  const locationState = asMessagesLocationState(location.state);
  const stateConversation = locationState?.conversation ?? null;
  const openContactId = locationState?.openContactId ?? null;
  const openContactPreview = locationState?.openContactPreview ?? null;

  const [fetchedConversation, setFetchedConversation] = useState<Conversation | null>(null);
  const [fetchLoading, setFetchLoading] = useState(false);
  const [fetchError, setFetchError] = useState<string | null>(null);

  const conversation = stateConversation ?? fetchedConversation;

  useEffect(() => {
    if (stateConversation || conversationId === null) {
      setFetchedConversation(null);
      setFetchLoading(false);
      setFetchError(conversationParam === undefined ? null : "Conversation not found.");
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
  }, [conversationId, conversationParam, stateConversation]);

  return (
    <>
      <ListColumn>
        <ConversationList
          selectedId={conversationId}
          onSelect={(c) =>
            navigate(`/messages/${c.id}`, {
              state: { conversation: c, openContactId, openContactPreview },
            })
          }
          query={query}
        />
      </ListColumn>
      <RightPane>
        <main className="min-h-0 min-w-0 flex-1 overflow-auto bg-bg text-text">
          {conversation ? (
            <MessageView
              conversation={conversation}
              onOpenContact={(contactId, preview) => {
                navigate(location.pathname + location.search, {
                  state: {
                    conversation,
                    openContactId: contactId,
                    openContactPreview: preview,
                  },
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
      </RightPane>
    </>
  );
}
