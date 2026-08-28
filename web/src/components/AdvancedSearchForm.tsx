import { useState } from "react";
import type { Key } from "react-aria-components";
import { popupShadow } from "../lib/uiStyles";
import {
  type ActivityFilter,
  type AdvancedSearchMode,
  buildContactsQuery,
  buildMessagesQuery,
  type CountFilterInput,
  canSubmitContacts,
  canSubmitMessages,
  type DateBoundFilter,
  EMPTY_COUNT,
  EMPTY_DATE_BOUND,
} from "./advancedSearch/buildAdvancedQuery";
import ContactsSearchFields from "./advancedSearch/ContactsSearchFields";
import MessagesSearchFields from "./advancedSearch/MessagesSearchFields";
import Button from "./Button";

export type { AdvancedSearchMode } from "./advancedSearch/buildAdvancedQuery";

export default function AdvancedSearchForm({
  mode,
  onApply,
  onClose,
  withTail = false,
}: {
  mode: AdvancedSearchMode;
  onApply: (query: string) => void;
  onClose: () => void;
  /** Gmail-style notch pointing at the contacts search icon. */
  withTail?: boolean;
}) {
  const [nameOrHandle, setNameOrHandle] = useState("");
  const [contactName, setContactName] = useState("");
  const [contactNameSaved, setContactNameSaved] = useState("");
  const [handle, setHandle] = useState("");
  const [msgType, setMsgType] = useState<"all" | "direct" | "group">("all");
  const [participants, setParticipants] = useState<CountFilterInput>(EMPTY_COUNT);
  const [firstMsgBound, setFirstMsgBound] = useState<DateBoundFilter>(EMPTY_DATE_BOUND);
  const [lastMsgBound, setLastMsgBound] = useState<DateBoundFilter>(EMPTY_DATE_BOUND);
  const [activity, setActivity] = useState<ActivityFilter>("any");
  const [noPreferredName, setNoPreferredName] = useState(false);
  const [noHandle, setNoHandle] = useState(false);
  const [handleSaved, setHandleSaved] = useState("");
  const [services, setServices] = useState<Key[]>([]);
  /** Snapshot restored when unchecking No handle (handle-dependent filters). */
  const [lockedByNoHandle, setLockedByNoHandle] = useState<{
    services: Key[];
    firstMsgBound: DateBoundFilter;
    lastMsgBound: DateBoundFilter;
    activity: ActivityFilter;
  } | null>(null);

  const canSubmit =
    mode === "messages"
      ? canSubmitMessages({ nameOrHandle, handle, msgType, participants })
      : canSubmitContacts({
          contactName,
          handle,
          firstMsgBound,
          lastMsgBound,
          activity,
          noPreferredName,
          noHandle,
          services,
        });

  const submit = () => {
    if (!canSubmit) return;
    // Persist trimmed values in the fields so the UI matches the query.
    if (mode === "messages") {
      setNameOrHandle((v) => v.trim());
      setHandle((v) => v.trim());
      onApply(
        buildMessagesQuery({
          nameOrHandle: nameOrHandle.trim(),
          handle: handle.trim(),
          msgType,
          participants,
        }),
      );
    } else {
      setContactName((v) => v.trim());
      setHandle((v) => v.trim());
      onApply(
        buildContactsQuery({
          contactName: contactName.trim(),
          handle: handle.trim(),
          firstMsgBound,
          lastMsgBound,
          activity,
          noPreferredName,
          noHandle,
          services,
        }),
      );
    }
  };

  return (
    <div className={`relative z-[70] rounded-md border border-border bg-panel p-3 ${popupShadow}`}>
      {withTail ? (
        <span
          aria-hidden
          className="pointer-events-none absolute -top-[6px] left-[1.15rem] z-[71] box-border h-3 w-3 rotate-45 border-l border-t border-border bg-panel"
        />
      ) : null}

      {mode === "messages" ? (
        <MessagesSearchFields
          nameOrHandle={nameOrHandle}
          onNameOrHandleChange={setNameOrHandle}
          handle={handle}
          onHandleChange={setHandle}
          msgType={msgType}
          onMsgTypeChange={setMsgType}
          participants={participants}
          onParticipantsChange={setParticipants}
        />
      ) : (
        <ContactsSearchFields
          contactName={contactName}
          onContactNameChange={setContactName}
          contactNameSaved={contactNameSaved}
          onContactNameSavedChange={setContactNameSaved}
          handle={handle}
          onHandleChange={setHandle}
          handleSaved={handleSaved}
          onHandleSavedChange={setHandleSaved}
          noPreferredName={noPreferredName}
          onNoPreferredNameChange={setNoPreferredName}
          noHandle={noHandle}
          onNoHandleChange={setNoHandle}
          services={services}
          onServicesChange={setServices}
          firstMsgBound={firstMsgBound}
          onFirstMsgBoundChange={setFirstMsgBound}
          lastMsgBound={lastMsgBound}
          onLastMsgBoundChange={setLastMsgBound}
          activity={activity}
          onActivityChange={setActivity}
          lockedByNoHandle={lockedByNoHandle}
          onLockedByNoHandleChange={setLockedByNoHandle}
        />
      )}

      <div className="mt-5 flex justify-start gap-2">
        <Button
          variant="primary"
          disabled={!canSubmit}
          onClick={submit}
          className="!py-1.5 !text-[0.813rem]"
        >
          Search
        </Button>
        <Button onClick={onClose} size="sm">
          Cancel
        </Button>
      </div>
    </div>
  );
}
