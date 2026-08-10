import { useState, type CSSProperties, type ReactNode } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { useAuth } from "../lib/auth";
import { isTauri } from "../lib/tauri-check";
import { listGroups, addGroup, removeGroup } from "../lib/savedGroups";
import SavedGroupForm from "./SavedGroupForm";

function NavIcon({ children }: { children: ReactNode }) {
  return (
    <svg
      width="15"
      height="15"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      style={{ flexShrink: 0 }}
    >
      {children}
    </svg>
  );
}

function ConversationsIcon() {
  return (
    <NavIcon>
      {/* Message bubble */}
      <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
    </NavIcon>
  );
}

function ContactsIcon() {
  return (
    <NavIcon>
      {/* Address book */}
      <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
      <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
      <circle cx="12" cy="8" r="2" />
      <path d="M9 14c0-1.1 1.3-2 3-2s3 .9 3 2" />
    </NavIcon>
  );
}

function TrashIcon() {
  return (
    <NavIcon>
      <path d="M3 6h18" />
      <path d="M8 6V4h8v2" />
      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
      <path d="M10 11v6" />
      <path d="M14 11v6" />
    </NavIcon>
  );
}

function ImportIcon() {
  return (
    <NavIcon>
      {/* Import: arrow into tray */}
      <path d="M12 3v12" />
      <path d="m8 11 4 4 4-4" />
      <path d="M4 19h16" />
    </NavIcon>
  );
}

function ExportIcon() {
  return (
    <NavIcon>
      {/* Export: arrow out of tray */}
      <path d="M12 15V3" />
      <path d="m8 7 4-4 4 4" />
      <path d="M4 19h16" />
    </NavIcon>
  );
}

export default function LeftPanel({
  onSearchChange,
  onSearch,
}: {
  onSearchChange: (v: string) => void;
  onSearch: (q: string) => void;
}) {
  const location = useLocation();
  const navigate = useNavigate();
  const { logout } = useAuth();

  function isActive(path: string): boolean {
    if (path === "/") return location.pathname === "/" || location.pathname.startsWith("/messages/");
    return location.pathname.startsWith(path);
  }

  const linkStyle = (active: boolean): CSSProperties => ({
    padding: "0.375rem 0.75rem",
    fontSize: "0.875rem",
    cursor: "pointer",
    borderRadius: "4px",
    background: active ? "var(--hover)" : "transparent",
    fontWeight: active ? 600 : 400,
    border: "none",
    textAlign: "left",
    width: "100%",
    display: "flex",
    alignItems: "center",
    gap: "0.5rem",
    color: "var(--text)",
  });

  const signOutStyle: CSSProperties = {
    ...linkStyle(false),
    background: "transparent",
    fontWeight: 400,
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
        <div
          style={{
            padding: "0.25rem 0.75rem 0.375rem",
            fontSize: "0.875rem",
            fontWeight: 700,
            color: "var(--text)",
          }}
        >
          Message Vault
        </div>
        <div style={{ paddingLeft: "0.75rem" }}>
          <button style={linkStyle(isActive("/"))} onClick={() => navigate("/")}>
            <ConversationsIcon />
            Conversations
          </button>
          <button style={linkStyle(isActive("/contacts"))} onClick={() => navigate("/contacts")}>
            <ContactsIcon />
            Contacts
          </button>
          <button style={linkStyle(isActive("/trash"))} onClick={() => navigate("/trash")}>
            <TrashIcon />
            Trash
          </button>
        </div>
      </div>

      {/* Import/Export — Tauri only */}
      {isTauri() && (
        <div style={{ padding: "0.5rem 0.75rem", borderTop: "1px solid var(--border)" }}>
          <div
            style={{
              padding: "0.25rem 0.75rem 0.375rem",
              fontSize: "0.875rem",
              fontWeight: 700,
              color: "var(--text)",
            }}
          >
            Messages
          </div>
          <div style={{ paddingLeft: "0.75rem" }}>
            <button style={linkStyle(isActive("/import"))} onClick={() => navigate("/import")}>
              <ImportIcon />
              Import
            </button>
            <button style={linkStyle(isActive("/export"))} onClick={() => navigate("/export")}>
              <ExportIcon />
              Export
            </button>
          </div>
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
        <button style={linkStyle(isActive("/settings"))} onClick={() => navigate("/settings")}>
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
