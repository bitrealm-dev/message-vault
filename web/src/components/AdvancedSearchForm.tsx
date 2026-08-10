import { useState } from "react";
import {
  Button as AriaButton,
  Label,
  ListBox,
  ListBoxItem,
  Popover,
  Select as RACSelect,
  type Key,
} from "react-aria-components";
import Button from "./Button";
import DateField from "./DateField";
import Select, { ListBoxItem as SelectListBoxItem, selectItemClassName } from "./Select";
import { popupShadow } from "../lib/uiStyles";

export type AdvancedSearchMode = "messages" | "contacts";

/** Same operators as web-next CountField (Any / Equal to / More than / Less than). */
type CountComparator = "=" | ">" | "<";
type CountFilterInput = {
  comparator: CountComparator | "any";
  value: string;
};

type ActivityFilter = "any" | "messages" | "no-messages";

const EMPTY_COUNT: CountFilterInput = { comparator: "any", value: "" };

const SERVICE_ITEMS = [
  { id: "imessage", name: "iMessage" },
  { id: "sms", name: "SMS/MMS" },
  { id: "whatsapp", name: "WhatsApp" },
] as const;

/** Compact text input for the filter grid (8px vertical to keep rows dense). */
const inputClass =
  "box-border w-full rounded-md border border-border bg-bg px-2 py-1 text-[0.813rem] text-text outline-none focus:border-accent";

/** Uppercase micro-label above each filter field. */
const labelClass =
  "mb-1 block text-[0.688rem] font-semibold uppercase tracking-[0.05em] text-muted";

const dateGroupClass =
  "box-border flex w-full min-w-0 items-center overflow-hidden rounded-md border border-border bg-bg px-2 py-1 focus-within:border-accent";

/** Select triggers in this panel — slightly squarer than the shared Select default. */
const selectTriggerClass = "!rounded-md !bg-bg";

/** Menu rows sized to match the compact field text (0.813rem), not the shared 0.875rem Select. */
function compactSelectItemClassName(state: {
  isFocused: boolean;
  isSelected: boolean;
}): string {
  return selectItemClassName(state).replace("text-[0.875rem]", "text-[0.813rem]");
}

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
  const [firstMsgDate, setFirstMsgDate] = useState("");
  const [lastMsgDate, setLastMsgDate] = useState("");
  const [activity, setActivity] = useState<ActivityFilter>("any");
  const [noPreferredName, setNoPreferredName] = useState(false);
  const [services, setServices] = useState<Key[]>([]);

  const buildQuery = (): string => {
    const parts: string[] = [];
    const push = (s: string) => {
      if (s.trim()) parts.push(s.trim());
    };
    if (mode === "messages") {
      // Conversation list API: free-text name/handle, is:, participants:, handle:.
      if (nameOrHandle.trim()) push(nameOrHandle.trim());
      if (handle.trim()) push(`handle:${handle.trim()}`);
      if (msgType === "direct") push("is:direct");
      if (msgType === "group") push("is:group");
      const participantCmp = composeCountComparison(participants);
      if (participantCmp) push(`participants:${participantCmp}`);
    } else {
      if (contactName.trim()) push(contactName.trim());
      if (handle.trim()) push(`handle:"${handle.trim()}"`);
      if (firstMsgDate) push(`first-contact:${firstMsgDate}`);
      if (lastMsgDate) push(`last-contact:${lastMsgDate}`);
      if (activity === "messages") push("has:messages");
      if (activity === "no-messages") push("has:no-messages");
      if (noPreferredName) push("has:no-name");
      for (const id of services) {
        push(`service:${String(id)}`);
      }
      push("search:contacts");
    }
    return parts.join(" ");
  };

  const canSubmit =
    mode === "messages"
      ? Boolean(
          nameOrHandle.trim() ||
            handle.trim() ||
            msgType !== "all" ||
            composeCountComparison(participants),
        )
      : Boolean(
          contactName.trim() ||
            handle.trim() ||
            firstMsgDate ||
            lastMsgDate ||
            activity !== "any" ||
            noPreferredName ||
            services.length > 0,
        );

  const submit = () => {
    if (!canSubmit) return;
    // Persist trimmed values in the fields so the UI matches the query.
    if (mode === "messages") {
      setNameOrHandle((v) => v.trim());
      setHandle((v) => v.trim());
    } else {
      setContactName((v) => v.trim());
      setHandle((v) => v.trim());
    }
    onApply(buildQuery());
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
              triggerClassName={selectTriggerClass}
            >
              <SelectListBoxItem id="all" className={compactSelectItemClassName}>All</SelectListBoxItem>
              <SelectListBoxItem id="direct" className={compactSelectItemClassName}>Direct</SelectListBoxItem>
              <SelectListBoxItem id="group" className={compactSelectItemClassName}>Group</SelectListBoxItem>
            </Select>
          </div>
          <CountField
            label="Group participants"
            value={participants}
            onChange={setParticipants}
          />
        </div>
      ) : (
        <div className="grid grid-cols-2 gap-x-3 gap-y-3">
          <div className="min-w-0">
            <label className={labelClass}>Name</label>
            <input
              className={`${inputClass} ${noPreferredName ? "cursor-not-allowed opacity-40" : ""}`}
              value={contactName}
              disabled={noPreferredName}
              onChange={(e) => setContactName(e.target.value)}
              placeholder={noPreferredName ? undefined : "Pat Lee"}
            />
            <label className="mt-2 flex cursor-pointer items-center gap-2 text-[0.813rem] text-text">
              <input
                type="checkbox"
                checked={noPreferredName}
                onChange={(e) => {
                  const checked = e.target.checked;
                  if (checked) {
                    setContactNameSaved(contactName);
                    setContactName("");
                    setNoPreferredName(true);
                  } else {
                    setContactName(contactNameSaved);
                    setContactNameSaved("");
                    setNoPreferredName(false);
                  }
                }}
                className="checkbox-list"
              />
              No name
            </label>
          </div>
          <div className="min-w-0">
            <label className={labelClass}>Handle</label>
            <input
              className={inputClass}
              value={handle}
              onChange={(e) => setHandle(e.target.value)}
              placeholder="+15555550100"
            />
          </div>
          <DateField
            label={
              <>
                First message{" "}
                <span className="normal-case tracking-normal font-medium">(On or After)</span>
              </>
            }
            pickAriaLabel="First message"
            value={firstMsgDate}
            onChange={setFirstMsgDate}
            labelClassName={labelClass}
            groupClassName={dateGroupClass}
            className="min-w-0 w-full overflow-hidden"
          />
          <DateField
            label={
              <>
                Last message{" "}
                <span className="normal-case tracking-normal font-medium">(Before)</span>
              </>
            }
            pickAriaLabel="Last message"
            value={lastMsgDate}
            onChange={setLastMsgDate}
            labelClassName={labelClass}
            groupClassName={dateGroupClass}
            className="min-w-0 w-full overflow-hidden"
          />
          <div className="min-w-0">
            <label className={labelClass}>Activity</label>
            <Select
              selectedKey={activity}
              onSelectionChange={(k) => setActivity(k as ActivityFilter)}
              aria-label="Activity"
              className="w-full min-w-0"
              triggerClassName={`!box-border !min-w-0 !px-2 !py-1 !text-[0.813rem] ${selectTriggerClass}`}
            >
              <SelectListBoxItem id="any" className={compactSelectItemClassName}>Any</SelectListBoxItem>
              <SelectListBoxItem id="messages" className={compactSelectItemClassName}>Has messages</SelectListBoxItem>
              <SelectListBoxItem id="no-messages" className={compactSelectItemClassName}>Never messaged</SelectListBoxItem>
            </Select>
          </div>
          <div className="min-w-0">
            <ServiceMultiSelect value={services} onChange={setServices} />
          </div>
        </div>
      )}

      <div className="mt-3 flex justify-start gap-2">
        <Button
          variant="primary"
          disabled={!canSubmit}
          onClick={submit}
          className="!py-1.5 !text-[0.813rem]"
        >
          Search
        </Button>
        <Button onClick={onClose} className="!px-3 !py-1.5 !text-[0.813rem]">Cancel</Button>
      </div>
    </div>
  );
}

/** Multi-select without search — click the whole field to open. */
function ServiceMultiSelect({
  value,
  onChange,
}: {
  value: Key[];
  onChange: (keys: Key[]) => void;
}) {
  const selectedLabels = SERVICE_ITEMS.filter((item) => value.includes(item.id))
    .map((item) => item.name)
    .join(", ");

  return (
    <RACSelect<object, "multiple">
      selectionMode="multiple"
      shouldCloseOnSelect={false}
      value={value}
      onChange={onChange}
      placeholder="Any"
      className="w-full min-w-0"
    >
      <Label className={labelClass}>Service</Label>
      <AriaButton className="box-border flex w-full min-w-0 items-center justify-between gap-2 overflow-hidden rounded-md border border-border bg-bg px-2 py-1 text-[0.813rem] text-text outline-none focus:border-accent">
        <span className="min-w-0 truncate text-muted">
          {value.length > 0 ? "Select…" : "Any"}
        </span>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden className="ml-1 shrink-0 text-muted">
          <path
            d="M2.5 3.5 5 6l2.5-2.5"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </AriaButton>
      {selectedLabels ? (
        <div className="mt-1 text-[0.75rem] leading-snug text-muted">{selectedLabels}</div>
      ) : null}
      <Popover
        data-mv-overlay=""
        className={`z-[100] min-w-[var(--trigger-width)] rounded-md border border-border bg-popover p-1 outline-none ${popupShadow}`}
      >
        <ListBox className="outline-none">
          {SERVICE_ITEMS.map((item) => (
            <ListBoxItem
              key={item.id}
              id={item.id}
              textValue={item.name}
              className={compactSelectItemClassName}
            >
              {({ isSelected }) => (
                <div className="flex items-center gap-2">
                  <span
                    aria-hidden
                    className={`inline-flex h-3.5 w-3.5 items-center justify-center rounded border text-[0.625rem] ${
                      isSelected
                        ? "border-accent bg-accent text-sent-text"
                        : "border-border bg-bg text-transparent"
                    }`}
                  >
                    ✓
                  </span>
                  {item.name}
                </div>
              )}
            </ListBoxItem>
          ))}
        </ListBox>
      </Popover>
    </RACSelect>
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
          triggerClassName={selectTriggerClass}
          onSelectionChange={(k) => {
            const comparator = k as CountComparator | "any";
            onChange({
              comparator,
              value: comparator === "any" ? "" : value.value,
            });
          }}
        >
          <SelectListBoxItem id="any" className={compactSelectItemClassName}>Any</SelectListBoxItem>
          <SelectListBoxItem id="=" className={compactSelectItemClassName}>Equal to</SelectListBoxItem>
          <SelectListBoxItem id=">" className={compactSelectItemClassName}>More than</SelectListBoxItem>
          <SelectListBoxItem id="<" className={compactSelectItemClassName}>Less than</SelectListBoxItem>
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
