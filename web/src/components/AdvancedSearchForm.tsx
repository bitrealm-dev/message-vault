import { useState } from "react";
import Button from "./Button";
import Select, { ListBoxItem, selectItemClassName } from "./Select";

export type AdvancedSearchMode = "messages" | "contacts";

/** Same operators as web-next CountField (Any / Equal to / More than / Less than). */
type CountComparator = "=" | ">" | "<";
type CountFilterInput = {
  comparator: CountComparator | "any";
  value: string;
};

const EMPTY_COUNT: CountFilterInput = { comparator: "any", value: "" };

/** Compact text input for the filter grid (8px vertical to keep rows dense). */
const inputClass =
  "box-border w-full rounded border border-border bg-bg px-2 py-1 text-[0.813rem] text-text";

/** Uppercase micro-label above each filter field. */
const labelClass =
  "mb-1 block text-[0.688rem] font-semibold uppercase tracking-[0.05em] text-muted";

function composeCountComparison(input: CountFilterInput): string | null {
  if (input.comparator === "any") return null;
  const value = input.value.trim();
  if (!/^\d+$/.test(value)) return null;
  return `${input.comparator}${value}`;
}

export default function AdvancedSearchForm({
  mode,
  onApply,
  onClose,
}: {
  mode: AdvancedSearchMode;
  onApply: (query: string) => void;
  onClose: () => void;
}) {
  const [nameOrHandle, setNameOrHandle] = useState("");
  const [handle, setHandle] = useState("");
  const [msgType, setMsgType] = useState<"all" | "direct" | "group">("all");
  const [participants, setParticipants] = useState<CountFilterInput>(EMPTY_COUNT);
  const [firstMsgDate, setFirstMsgDate] = useState("");
  const [lastMsgDate, setLastMsgDate] = useState("");
  const [msgCount, setMsgCount] = useState<CountFilterInput>(EMPTY_COUNT);
  const [groupCount, setGroupCount] = useState<CountFilterInput>(EMPTY_COUNT);

  const buildQuery = (): string => {
    const parts: string[] = [];
    const push = (s: string) => { if (s.trim()) parts.push(s.trim()); };
    if (mode === "messages") {
      // Conversation list API: free-text name/handle, is:, participants:, handle:.
      if (nameOrHandle) push(nameOrHandle.trim());
      if (handle) push(`handle:${handle.trim()}`);
      if (msgType === "direct") push("is:direct");
      if (msgType === "group") push("is:group");
      const participantCmp = composeCountComparison(participants);
      if (participantCmp) push(`participants:${participantCmp}`);
    } else {
      if (handle) push(`handle:"${handle}"`);
      if (firstMsgDate) push(`first-contact:${firstMsgDate}`);
      if (lastMsgDate) push(`last-contact:${lastMsgDate}`);
      const messageCmp = composeCountComparison(msgCount);
      if (messageCmp) push(`message-count:${messageCmp}`);
      const groupCmp = composeCountComparison(groupCount);
      if (groupCmp) push(`group-count:${groupCmp}`);
      push("search:contacts");
    }
    return parts.join(" ");
  };

  return (
    <div className="z-[60] rounded-md border border-border bg-panel p-3 shadow-[0_4px_12px_rgba(0,0,0,0.1)]">
      <div className="mb-3 flex items-center gap-2">
        <span className="text-[0.813rem] font-semibold text-text">
          {mode === "messages" ? "Conversation filters" : "Contact filters"}
        </span>
        <span className="flex-1" />
        <button type="button" onClick={onClose} className="cursor-pointer border-none bg-none text-[1rem] text-muted">×</button>
      </div>

      {mode === "messages" ? (
        <div className="grid grid-cols-2 gap-3">
          <div className="col-span-2">
            <label className={labelClass}>Name or title</label>
            <input
              className={inputClass}
              value={nameOrHandle}
              onChange={(e) => setNameOrHandle(e.target.value)}
              placeholder="Gregory Coleman"
            />
          </div>
          <div>
            <label className={labelClass}>Handle</label>
            <input
              className={inputClass}
              value={handle}
              onChange={(e) => setHandle(e.target.value)}
              placeholder="+15555550100"
            />
          </div>
          <div>
            <label className={labelClass}>Conversation type</label>
            <Select
              selectedKey={msgType}
              onSelectionChange={(k) => setMsgType(k as "all" | "direct" | "group")}
              aria-label="Conversation type"
              triggerClassName="!bg-bg"
            >
              <ListBoxItem id="all" className={selectItemClassName}>All</ListBoxItem>
              <ListBoxItem id="direct" className={selectItemClassName}>Direct</ListBoxItem>
              <ListBoxItem id="group" className={selectItemClassName}>Group</ListBoxItem>
            </Select>
          </div>
          <CountField
            label="Group participants"
            value={participants}
            onChange={setParticipants}
          />
        </div>
      ) : (
        <div className="grid grid-cols-2 gap-3">
          <div><label className={labelClass}>Handle</label><input className={inputClass} value={handle} onChange={(e) => setHandle(e.target.value)} placeholder="bob#1234" /></div>
          <div><label className={labelClass}>First message date from</label><input type="date" className={inputClass} value={firstMsgDate} onChange={(e) => setFirstMsgDate(e.target.value)} /></div>
          <div><label className={labelClass}>Last message date to</label><input type="date" className={inputClass} value={lastMsgDate} onChange={(e) => setLastMsgDate(e.target.value)} /></div>
          <CountField
            label="Direct message count"
            value={msgCount}
            onChange={setMsgCount}
          />
          <CountField
            label="Group message count"
            value={groupCount}
            onChange={setGroupCount}
          />
        </div>
      )}

      <div className="mt-3 flex justify-end gap-2">
        <Button onClick={onClose} className="!px-3 !py-1.5 !text-[0.813rem]">Cancel</Button>
        <Button variant="primary" onClick={() => onApply(buildQuery())} className="!py-1.5 !text-[0.813rem]">Apply</Button>
      </div>
    </div>
  );
}

function CountField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: CountFilterInput;
  onChange: (next: CountFilterInput) => void;
}) {
  return (
    <div>
      <label className={labelClass}>{label}</label>
      <div className="grid grid-cols-[7rem_minmax(0,1fr)] gap-1.5">
        <Select
          selectedKey={value.comparator}
          aria-label={`${label} comparison`}
          triggerClassName="!bg-bg"
          onSelectionChange={(k) => {
            const comparator = k as CountComparator | "any";
            onChange({
              comparator,
              value: comparator === "any" ? "" : value.value,
            });
          }}
        >
          <ListBoxItem id="any" className={selectItemClassName}>Any</ListBoxItem>
          <ListBoxItem id="=" className={selectItemClassName}>Equal to</ListBoxItem>
          <ListBoxItem id=">" className={selectItemClassName}>More than</ListBoxItem>
          <ListBoxItem id="<" className={selectItemClassName}>Less than</ListBoxItem>
        </Select>
        <input
          type="number"
          min={0}
          step={1}
          className={`${inputClass} ${value.comparator === "any" ? "opacity-40" : ""}`}
          value={value.comparator === "any" ? "" : value.value}
          disabled={value.comparator === "any"}
          aria-label={`${label} value`}
          onChange={(e) => onChange({ ...value, value: e.target.value })}
        />
      </div>
    </div>
  );
}
