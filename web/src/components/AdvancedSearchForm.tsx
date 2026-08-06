import { useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { apiClient } from "../lib/api";

export default function AdvancedSearchForm({
  onApply,
  onClose,
}: {
  onApply: (query: string) => void;
  onClose: () => void;
}) {
  const [tab, setTab] = useState<"messages" | "contacts">("messages");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [withPerson, setWithPerson] = useState("");
  const [hasWords, setHasWords] = useState("");
  const [notWords, setNotWords] = useState("");
  const [subject, setSubject] = useState("");
  const [dateFrom, setDateFrom] = useState("");
  const [dateTo, setDateTo] = useState("");
  const [msgType, setMsgType] = useState<"all" | "direct" | "group">("all");
  const [source, setSource] = useState("");
  const [sources, setSources] = useState<string[]>([]);
  const [handle, setHandle] = useState("");
  const [firstMsgDate, setFirstMsgDate] = useState("");
  const [lastMsgDate, setLastMsgDate] = useState("");
  const [msgCount, setMsgCount] = useState("");
  const [groupCount, setGroupCount] = useState("");

  useEffect(() => {
    apiClient
      .get<{ sources: string[] }>("/v1/auth/check")
      .then((res) => setSources(res.sources || []))
      .catch(() => setSources([]));
  }, []);

  const inputStyle: CSSProperties = {
    width: "100%", padding: "0.25rem 0.5rem", fontSize: "0.813rem",
    border: "1px solid #d1d5db", borderRadius: "4px", boxSizing: "border-box",
  };
  const labelStyle: CSSProperties = {
    fontSize: "0.688rem", fontWeight: 600, color: "#6b7280",
    textTransform: "uppercase", letterSpacing: "0.05em", marginBottom: "0.25rem",
    display: "block",
  };

  const buildQuery = (): string => {
    const parts: string[] = [];
    const push = (s: string) => { if (s.trim()) parts.push(s.trim()); };
    if (tab === "messages") {
      if (from) push(`from:"${from}"`);
      if (to) push(`to:"${to}"`);
      if (withPerson) push(`with:"${withPerson}"`);
      if (hasWords) push(hasWords.trim());
      if (notWords) push(notWords.trim().split(/\s+/).map((w) => `-${w}`).join(" "));
      if (subject) push(`subject:"${subject}"`);
      if (dateFrom) push(`after:${dateFrom}`);
      if (dateTo) push(`before:${dateTo}`);
      if (msgType === "direct") push("is:direct");
      if (msgType === "group") push("is:group");
      if (source) push(`source:${source}`);
    } else {
      if (handle) push(`handle:"${handle}"`);
      if (firstMsgDate) push(`first-contact:${firstMsgDate}`);
      if (lastMsgDate) push(`last-contact:${lastMsgDate}`);
      if (msgCount) push(`message-count:${msgCount}`);
      if (groupCount) push(`group-count:${groupCount}`);
      push("search:contacts");
    }
    return parts.join(" ");
  };

  return (
    <div style={{
      background: "#fff", border: "1px solid #e5e7eb", borderRadius: "6px",
      boxShadow: "0 4px 12px rgba(0,0,0,0.1)", padding: "0.75rem", zIndex: 60,
    }}>
      <div style={{ display: "flex", gap: "0.5rem", marginBottom: "0.75rem" }}>
        <button onClick={() => setTab("messages")}
          style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem", fontWeight: tab === "messages" ? 600 : 400 }}>
          Messages
        </button>
        <button onClick={() => setTab("contacts")}
          style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem", fontWeight: tab === "contacts" ? 600 : 400 }}>
          Contacts
        </button>
        <span style={{ flex: 1 }} />
        <button onClick={onClose} style={{ border: "none", background: "none", fontSize: "1rem", cursor: "pointer" }}>×</button>
      </div>

      {tab === "messages" ? (
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.75rem" }}>
          <div><label style={labelStyle}>From</label><input style={inputStyle} value={from} onChange={(e) => setFrom(e.target.value)} placeholder="Name or handle" /></div>
          <div><label style={labelStyle}>To</label><input style={inputStyle} value={to} onChange={(e) => setTo(e.target.value)} placeholder="Name or handle" /></div>
          <div><label style={labelStyle}>With person</label><input style={inputStyle} value={withPerson} onChange={(e) => setWithPerson(e.target.value)} placeholder="Name or handle" /></div>
          <div><label style={labelStyle}>Has words</label><input style={inputStyle} value={hasWords} onChange={(e) => setHasWords(e.target.value)} placeholder="vacation beach" /></div>
          <div><label style={labelStyle}>Doesn't have words</label><input style={inputStyle} value={notWords} onChange={(e) => setNotWords(e.target.value)} placeholder="work meeting" /></div>
          <div><label style={labelStyle}>Subject</label><input style={inputStyle} value={subject} onChange={(e) => setSubject(e.target.value)} /></div>
          <div><label style={labelStyle}>Date from</label><input type="date" style={inputStyle} value={dateFrom} onChange={(e) => setDateFrom(e.target.value)} /></div>
          <div><label style={labelStyle}>Date to</label><input type="date" style={inputStyle} value={dateTo} onChange={(e) => setDateTo(e.target.value)} /></div>
          <div><label style={labelStyle}>Message type</label><select style={inputStyle} value={msgType} onChange={(e) => setMsgType(e.target.value as "all" | "direct" | "group")}>
            <option value="all">All</option><option value="direct">Direct</option><option value="group">Group</option>
          </select></div>
          <div><label style={labelStyle}>Source</label><select style={inputStyle} value={source} onChange={(e) => setSource(e.target.value)}>
            <option value="">Any</option>{sources.map((s) => <option key={s} value={s}>{s}</option>)}
          </select></div>
        </div>
      ) : (
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.75rem" }}>
          <div><label style={labelStyle}>Handle</label><input style={inputStyle} value={handle} onChange={(e) => setHandle(e.target.value)} placeholder="bob#1234" /></div>
          <div><label style={labelStyle}>First message date from</label><input type="date" style={inputStyle} value={firstMsgDate} onChange={(e) => setFirstMsgDate(e.target.value)} /></div>
          <div><label style={labelStyle}>Last message date to</label><input type="date" style={inputStyle} value={lastMsgDate} onChange={(e) => setLastMsgDate(e.target.value)} /></div>
          <div><label style={labelStyle}>Message count</label><input type="number" style={inputStyle} value={msgCount} onChange={(e) => setMsgCount(e.target.value)} placeholder="e.g. 1000" /></div>
          <div><label style={labelStyle}>Group conversation count</label><input type="number" style={inputStyle} value={groupCount} onChange={(e) => setGroupCount(e.target.value)} placeholder="e.g. 3" /></div>
        </div>
      )}

      <div style={{ display: "flex", gap: "0.5rem", justifyContent: "flex-end", marginTop: "0.75rem" }}>
        <button onClick={onClose} style={{ padding: "0.375rem 0.75rem", fontSize: "0.813rem" }}>Cancel</button>
        <button onClick={() => onApply(buildQuery())} style={{ padding: "0.375rem 1rem", fontSize: "0.813rem", fontWeight: 600 }}>Apply</button>
      </div>
    </div>
  );
}
