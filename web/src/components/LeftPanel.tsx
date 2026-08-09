import { useState } from "react";
import { useAuth } from "../lib/auth";
import { isTauri } from "../lib/tauri-check";
import { listGroups, addGroup, removeGroup } from "../lib/savedGroups";
import type { ActiveView } from "../lib/views";
import SavedGroupForm from "./SavedGroupForm";

export default function LeftPanel({
  activeView,
  onNavigate,
  onSearchChange,
  onSearch,
}: {
  activeView: ActiveView;
  onNavigate: (view: ActiveView) => void;
  onSearchChange: (v: string) => void;
  onSearch: (q: string) => void;
}) {
  const { logout } = useAuth();

  const linkStyle = (view: ActiveView) => ({
    padding: "0.375rem 0.75rem",
    fontSize: "0.875rem",
    cursor: "pointer",
    borderRadius: "4px",
    background: activeView === view ? "var(--hover)" : "transparent",
    fontWeight: activeView === view ? 600 : (400 as const),
    border: "none",
    textAlign: "left" as const,
    width: "100%",
    display: "block",
    color: "var(--text)",
  });

  const signOutStyle = {
    ...linkStyle("settings"),
    background: "transparent",
    fontWeight: 400 as const,
    color: "var(--danger)",
    marginTop: "0.25rem",
  };

  const [groups, setGroups] = useState(() => listGroups());
  const [showGroupForm, setShowGroupForm] = useState(false);

  return (
    <div style={{
      width: "220px", flexShrink: 0, borderRight: "1px solid var(--border)",
      background: "var(--panel)", display: "flex", flexDirection: "column",
      height: "100vh", overflow: "auto", color: "var(--text)",
    }}>
      {/* Browse */}
      <div style={{ padding: "0.5rem 0.75rem" }}>
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
        <div style={{ padding: "0.5rem 0.75rem", borderTop: "1px solid var(--border)" }}>
          <button style={linkStyle("import")} onClick={() => onNavigate("import")}>
            Import
          </button>
          <button style={linkStyle("export")} onClick={() => onNavigate("export")}>
            Export
          </button>
        </div>
      )}

      <div style={{ borderTop: "1px solid var(--border)", margin: "0 0.75rem" }} />

      {/* Saved groups */}
      <div style={{ padding: "0.75rem", flex: 1, minHeight: 0, overflow: "auto" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.25rem" }}>
          <span style={{ fontSize: "0.688rem", fontWeight: 600, color: "var(--muted)", textTransform: "uppercase", letterSpacing: "0.05em" }}>
            Saved Groups
          </span>
          <button
            onClick={() => setShowGroupForm(true)}
            style={{ fontSize: "0.688rem", border: "none", background: "none", color: "var(--accent)", cursor: "pointer", padding: 0 }}
          >
            + New
          </button>
        </div>
        {groups.length === 0 ? (
          <div style={{ fontSize: "0.813rem", color: "var(--muted)", padding: "0.25rem 0" }}>No saved groups</div>
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
                  cursor: "pointer", color: "var(--text)", overflow: "hidden",
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
                style={{ border: "none", background: "none", color: "var(--muted)", cursor: "pointer", fontSize: "0.75rem", padding: "0 0.25rem", flexShrink: 0 }}
              >
                ×
              </button>
            </div>
          ))
        )}
      </div>

      {/* Settings */}
      <div style={{ padding: "0.5rem 0.75rem", borderTop: "1px solid var(--border)" }}>
        <button style={linkStyle("settings")} onClick={() => onNavigate("settings")}>
          Settings
        </button>
        <button onClick={logout} style={signOutStyle}>
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
