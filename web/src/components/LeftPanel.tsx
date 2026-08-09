import { useState } from "react";
import { useAuth } from "../lib/auth";
import { isTauri } from "../lib/tauri-check";
import { listGroups, addGroup, removeGroup } from "../lib/savedGroups";
import SavedGroupForm from "./SavedGroupForm";

export default function LeftPanel({
  activeView,
  onNavigate,
  onSearchChange,
  onSearch,
}: {
  activeView: string;
  onNavigate: (view: string) => void;
  onSearchChange: (v: string) => void;
  onSearch: (q: string) => void;
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
  const [showExportPopover, setShowExportPopover] = useState(false);

  return (
    <div style={{
      width: "220px", flexShrink: 0, borderRight: "1px solid #e5e7eb",
      background: "#f9fafb", display: "flex", flexDirection: "column",
      height: "100vh", overflow: "auto",
    }}>
      {/* Saved groups */}
      <div style={{ padding: "0.75rem", marginBottom: "0.5rem" }}>
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

      <div style={{ flex: 1 }} />

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
          <div style={{ position: "relative" }}>
            <button
              onClick={() => setShowExportPopover(!showExportPopover)}
              style={{ width: "100%", padding: "0.5rem", fontSize: "0.875rem" }}
            >
              Export
            </button>
            {showExportPopover && (
              <div style={{
                position: "absolute", bottom: "100%", left: 0, right: 0,
                background: "#fff", border: "1px solid #d1d5db", borderRadius: "6px",
                boxShadow: "0 4px 12px rgba(0,0,0,0.15)", zIndex: 80, padding: "0.25rem",
              }}>
                <button onClick={() => { onNavigate("export"); setShowExportPopover(false); }}
                  style={{ display: "block", width: "100%", textAlign: "left", border: "none", background: "transparent", padding: "0.375rem 0.5rem", fontSize: "0.813rem", cursor: "pointer", borderRadius: "4px" }}>
                  Export entire vault
                </button>
                <button onClick={() => { onNavigate("export"); setShowExportPopover(false); }}
                  style={{ display: "block", width: "100%", textAlign: "left", border: "none", background: "transparent", padding: "0.375rem 0.5rem", fontSize: "0.813rem", cursor: "pointer", borderRadius: "4px" }}>
                  Export current view
                </button>
                <button onClick={() => { onNavigate("export"); setShowExportPopover(false); }}
                  style={{ display: "block", width: "100%", textAlign: "left", border: "none", background: "transparent", padding: "0.375rem 0.5rem", fontSize: "0.813rem", cursor: "pointer", borderRadius: "4px" }}>
                  Export selected
                </button>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Settings */}
      <div style={{ padding: "0.5rem 0.75rem", borderTop: "1px solid #e5e7eb" }}>
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
