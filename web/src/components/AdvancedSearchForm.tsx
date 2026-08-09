import { useState } from "react";
import type { CSSProperties } from "react";
import Button from "./Button";

export type AdvancedSearchMode = "messages" | "contacts";

/** Same operators as web-next CountField (Any / Equal to / More than / Less than). */
type CountComparator = "=" | ">" | "<";
type CountFilterInput = {
  comparator: CountComparator | "any";
  value: string;
};

const EMPTY_COUNT: CountFilterInput = { comparator: "any", value: "" };

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

  const inputStyle: CSSProperties = {
    width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.813rem",
    border: "1px solid var(--border)", borderRadius: "4px", boxSizing: "border-box",
    background: "var(--bg)", color: "var(--text)",
  };
  const labelStyle: CSSProperties = {
    fontSize: "0.688rem", fontWeight: 600, color: "var(--muted)",
    textTransform: "uppercase", letterSpacing: "0.05em", marginBottom: "0.25rem",
    display: "block",
  };

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
    <div style={{
      background: "var(--panel)", border: "1px solid var(--border)", borderRadius: "6px",
      boxShadow: "0 4px 12px rgba(0,0,0,0.1)", padding: "0.75rem", zIndex: 60,
    }}>
      <div style={{ display: "flex", gap: "0.5rem", marginBottom: "0.75rem", alignItems: "center" }}>
        <span style={{ fontSize: "0.813rem", fontWeight: 600, color: "var(--text)" }}>
          {mode === "messages" ? "Conversation filters" : "Contact filters"}
        </span>
        <span style={{ flex: 1 }} />
        <button type="button" onClick={onClose} style={{ border: "none", background: "none", fontSize: "1rem", cursor: "pointer", color: "var(--muted)" }}>×</button>
      </div>

      {mode === "messages" ? (
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.75rem" }}>
          <div style={{ gridColumn: "1 / -1" }}>
            <label style={labelStyle}>Name or title</label>
            <input
              style={inputStyle}
              value={nameOrHandle}
              onChange={(e) => setNameOrHandle(e.target.value)}
              placeholder="Gregory Coleman"
            />
          </div>
          <div>
            <label style={labelStyle}>Handle</label>
            <input
              style={inputStyle}
              value={handle}
              onChange={(e) => setHandle(e.target.value)}
              placeholder="+15555550100"
            />
          </div>
          <div>
            <label style={labelStyle}>Conversation type</label>
            <select
              style={inputStyle}
              value={msgType}
              onChange={(e) => setMsgType(e.target.value as "all" | "direct" | "group")}
            >
              <option value="all">All</option>
              <option value="direct">Direct</option>
              <option value="group">Group</option>
            </select>
          </div>
          <CountField
            label="Group participants"
            value={participants}
            onChange={setParticipants}
            inputStyle={inputStyle}
            labelStyle={labelStyle}
          />
        </div>
      ) : (
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.75rem" }}>
          <div><label style={labelStyle}>Handle</label><input style={inputStyle} value={handle} onChange={(e) => setHandle(e.target.value)} placeholder="bob#1234" /></div>
          <div><label style={labelStyle}>First message date from</label><input type="date" style={inputStyle} value={firstMsgDate} onChange={(e) => setFirstMsgDate(e.target.value)} /></div>
          <div><label style={labelStyle}>Last message date to</label><input type="date" style={inputStyle} value={lastMsgDate} onChange={(e) => setLastMsgDate(e.target.value)} /></div>
          <CountField
            label="Direct message count"
            value={msgCount}
            onChange={setMsgCount}
            inputStyle={inputStyle}
            labelStyle={labelStyle}
          />
          <CountField
            label="Group message count"
            value={groupCount}
            onChange={setGroupCount}
            inputStyle={inputStyle}
            labelStyle={labelStyle}
          />
        </div>
      )}

      <div style={{ display: "flex", gap: "0.5rem", justifyContent: "flex-end", marginTop: "0.75rem" }}>
        <Button onClick={onClose} style={{ padding: "0.375rem 0.75rem", fontSize: "0.813rem" }}>Cancel</Button>
        <Button variant="primary" onClick={() => onApply(buildQuery())} style={{ padding: "0.375rem 1rem", fontSize: "0.813rem" }}>Apply</Button>
      </div>
    </div>
  );
}

function CountField({
  label,
  value,
  onChange,
  inputStyle,
  labelStyle,
}: {
  label: string;
  value: CountFilterInput;
  onChange: (next: CountFilterInput) => void;
  inputStyle: CSSProperties;
  labelStyle: CSSProperties;
}) {
  return (
    <div>
      <label style={labelStyle}>{label}</label>
      <div style={{ display: "grid", gridTemplateColumns: "7rem minmax(0, 1fr)", gap: "0.375rem" }}>
        <select
          style={inputStyle}
          value={value.comparator}
          aria-label={`${label} comparison`}
          onChange={(e) => {
            const comparator = e.target.value as CountComparator | "any";
            onChange({
              comparator,
              value: comparator === "any" ? "" : value.value,
            });
          }}
        >
          <option value="any">Any</option>
          <option value="=">Equal to</option>
          <option value=">">More than</option>
          <option value="<">Less than</option>
        </select>
        <input
          type="number"
          min={0}
          step={1}
          style={{
            ...inputStyle,
            opacity: value.comparator === "any" ? 0.4 : 1,
          }}
          value={value.comparator === "any" ? "" : value.value}
          disabled={value.comparator === "any"}
          aria-label={`${label} value`}
          onChange={(e) => onChange({ ...value, value: e.target.value })}
        />
      </div>
    </div>
  );
}
