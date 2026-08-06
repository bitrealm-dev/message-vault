import type { ReactNode } from "react";
import { useState } from "react";
import { useAuth } from "../lib/auth";
import { isTauri } from "../lib/tauri-check";
import { listGroups, addGroup, removeGroup } from "../lib/savedGroups";
import SavedGroupForm from "./SavedGroupForm";

export default function LeftPanel({
  activeView,
  onNavigate,
  searchQuery,
  onSearchChange,
  onSearch,
  conversationList,
}: {
  activeView: string;
  onNavigate: (view: string) => void;
  searchQuery: string;
  onSearchChange: (v: string) => void;
  onSearch: (q: string) => void;
  conversationList?: ReactNode;
}) {
  const { logout } = useAuth();

  const linkStyle = (view: string) => ({
    padding: "0.375rem 0.75rem",
    fontSize: "0.875rem",
    cursor: "pointer",
    borderRadius: "4px",
    background: activeView === view ? "#e5e7eb" : "transparent",
    fontWeight: activeView === view ? 600 : (400 as const),
    border: "none",
    textAlign: "left" as const,
    width: "100%",
    display: "block",
    color: "#1f2937",
  });

  const [groups, setGroups] = useState(() => listGroups());
  const [showGroupForm, setShowGroupForm] = useState(false);

  return (
    <div style={{
      width: "220px", flexShrink: 0, borderRight: "1px solid #e5e7eb",
      background: "#f9fafb", display: "flex", flexDirection: "column",
      height: "100vh", overflow: "auto",
    }}>
      {/* Global search */}
      <div style={{ padding: "0.75rem" }}>
        <input
          type="search"
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") onSearch(searchQuery);
          }}
          placeholder="Search vault"
          style={{
            width: "100%", padding: "0.375rem 0.5rem", fontSize: "0.813rem",
            border: "1px solid #d1d5db", borderRadius: "4px",
          }}
        />
      </div>

      {/* Saved groups */}
      <div style={{ padding: "0 0.75rem", marginBottom: "0.5rem" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.25rem" }}>
          <span style={{ fontSize: "0.688rem", fontWeight: 600, color: "#9ca3af", textTransform: "uppercase", letterSpacing: "0.05em" }}>
            Saved Groups
          </span>
          <button
            onClick={() => setShowGroupForm(true)}
            style={{ fontSize: "0.688rem", border: "none", background: "none", color: "#2563eb", cursor: "pointer", padding: 0 }}
          >
            + New
          </button>
        </div>
        {groups.length === 0 ? (
          <div style={{ fontSize: "0.813rem", color: "#9ca3af", padding: "0.25rem 0" }}>No saved groups</div>
        ) : (
          groups.map((g) => (
            <div key={g.id} style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
              <button
                onClick={() => {
                  onSearchChange(g.query);
                  onSearch(g.query);
                }}
                style={{
                  display: "block", flex: 1, textAlign: "left", border: "none",
                  background: "transparent", padding: "0.25rem 0", fontSize: "0.813rem",
                  cursor: "pointer", color: "#374151", overflow: "hidden",
                  textOverflow: "ellipsis", whiteSpace: "nowrap",
                }}
              >
                {g.name}
              </button>
              <button
                onClick={() => {
                  removeGroup(g.id);
                  setGroups(listGroups());
                }}
                title="Delete saved group"
                style={{ border: "none", background: "none", color: "#9ca3af", cursor: "pointer", fontSize: "0.75rem", padding: "0 0.25rem", flexShrink: 0 }}
              >
                ×
              </button>
            </div>
          ))
        )}
      </div>

      <div style={{ borderTop: "1px solid #e5e7eb", margin: "0 0.75rem" }} />

      {/* Conversation list slot */}
      {conversationList && (
        <div style={{ flex: 1, overflow: "hidden", display: "flex", flexDirection: "column" }}>
          {conversationList}
        </div>
      )}

      {/* Navigation */}
      <div style={{ padding: "0.5rem 0.75rem", borderTop: "1px solid #e5e7eb" }}>
        <button style={linkStyle("conversations")} onClick={() => onNavigate("conversations")}>
          Conversations
        </button>
        <button style={linkStyle("contacts")} onClick={() => onNavigate("contacts")}>
          Contacts
        </button>
        <button style={linkStyle("trash")} onClick={() => onNavigate("trash")}>
          Trash
        </button>
      </div>

      {/* Import/Export — Tauri only */}
      {isTauri() && (
        <div style={{ padding: "0.5rem 0.75rem", borderTop: "1px solid #e5e7eb" }}>
          <button
            onClick={() => onNavigate("import")}
            style={{
              width: "100%", padding: "0.5rem", marginBottom: "0.375rem",
              fontSize: "0.875rem", fontWeight: 600,
            }}
          >
            Import
          </button>
          <button
            onClick={() => onNavigate("export")}
            style={{ width: "100%", padding: "0.5rem", fontSize: "0.875rem" }}
          >
            Export
          </button>
        </div>
      )}

      {/* Profile + Settings */}
      <div style={{ padding: "0.5rem 0.75rem", borderTop: "1px solid #e5e7eb" }}>
        <button style={linkStyle("profile")} onClick={() => onNavigate("profile")}>
          Profile
        </button>
        <button style={linkStyle("settings")} onClick={() => onNavigate("settings")}>
          Settings
        </button>
        <button
          onClick={logout}
          style={{
            ...linkStyle(""),
            color: "#991b1b",
            marginTop: "0.25rem",
          }}
        >
          Sign out
        </button>
      </div>

      {/* Saved group form modal */}
      {showGroupForm && (
        <SavedGroupForm
          onSave={(name, query) => {
            addGroup(name, query);
            setGroups(listGroups());
            setShowGroupForm(false);
          }}
          onCancel={() => setShowGroupForm(false)}
        />
      )}
    </div>
  );
}
