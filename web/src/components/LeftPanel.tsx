import { useState, type ReactNode } from "react";
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
      className="shrink-0"
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

/** Sidebar header for each nav group (padding 0.25rem 0.75rem 0.375rem). */
const sectionHeaderClass = "px-3 pt-1 pb-1.5 text-[0.875rem] font-bold text-text";

/** Nav button: active section gets a hover-tinted background and semibold text. */
function linkClass(active: boolean): string {
  return `box-border flex w-full cursor-pointer items-center gap-2 rounded border-none bg-transparent px-3 py-1.5 text-left text-[0.875rem] text-text ${
    active ? "bg-hover font-semibold" : "font-normal"
  }`;
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

  const signOutClass = `${linkClass(false)} mt-1 font-normal text-danger`;

  const [groups, setGroups] = useState(() => listGroups());
  const [showGroupForm, setShowGroupForm] = useState(false);

  return (
    <div className="flex h-screen w-[220px] shrink-0 flex-col overflow-auto border-r border-border bg-panel text-text">
      {/* Browse */}
      <div className="px-3 py-2">
        <div className={sectionHeaderClass}>Message Vault</div>
        <div className="pl-3">
          <button className={linkClass(isActive("/"))} onClick={() => navigate("/")}>
            <ConversationsIcon />
            Conversations
          </button>
          <button className={linkClass(isActive("/contacts"))} onClick={() => navigate("/contacts")}>
            <ContactsIcon />
            Contacts
          </button>
          <button className={linkClass(isActive("/trash"))} onClick={() => navigate("/trash")}>
            <TrashIcon />
            Trash
          </button>
        </div>
      </div>

      {/* Import/Export — Tauri only */}
      {isTauri() && (
        <div className="border-t border-border px-3 py-2">
          <div className={sectionHeaderClass}>Messages</div>
          <div className="pl-3">
            <button className={linkClass(isActive("/import"))} onClick={() => navigate("/import")}>
              <ImportIcon />
              Import
            </button>
            <button className={linkClass(isActive("/export"))} onClick={() => navigate("/export")}>
              <ExportIcon />
              Export
            </button>
          </div>
        </div>
      )}

      <div className="mx-3 border-t border-border" />

      {/* Saved groups */}
      <div className="min-h-0 flex-1 overflow-auto p-3">
        <div className="mb-1 flex items-center justify-between">
          <span className="text-[0.688rem] font-semibold uppercase tracking-[0.05em] text-muted">
            Saved Groups
          </span>
          <button
            onClick={() => setShowGroupForm(true)}
            className="cursor-pointer border-none bg-none p-0 text-[0.688rem] text-accent"
          >
            + New
          </button>
        </div>
        {groups.length === 0 ? (
          <div className="py-1 text-[0.813rem] text-muted">No saved groups</div>
        ) : (
          groups.map((g) => (
            <div key={g.id} className="flex items-center justify-between">
              <button
                onClick={() => {
                  onSearchChange(g.query);
                  onSearch(g.query);
                }}
                className="block flex-1 cursor-pointer truncate border-none bg-transparent py-1 text-left text-[0.813rem] text-text"
              >
                {g.name}
              </button>
              <button
                onClick={() => {
                  removeGroup(g.id);
                  setGroups(listGroups());
                }}
                title="Delete saved group"
                className="shrink-0 cursor-pointer border-none bg-none px-1 text-[0.75rem] text-muted"
              >
                ×
              </button>
            </div>
          ))
        )}
      </div>

      {/* Settings */}
      <div className="border-t border-border px-3 py-2">
        <button className={linkClass(isActive("/settings"))} onClick={() => navigate("/settings")}>
          Settings
        </button>
        <button onClick={logout} className={signOutClass}>
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
