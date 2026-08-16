import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import {
  createContactGroup,
  deleteContactGroup,
  groupSlug,
  isReservedGroupName,
  renameContactGroup,
  reservedGroupError,
} from "../lib/contactGroups";
import GroupNameDialog from "./GroupNameDialog";
import NavCollapsibleSection from "./NavCollapsibleSection";
import { EllipsisIcon, PeopleGroupIcon, PersonIcon } from "./icons";

function navRowClass(active: boolean): string {
  return `group relative flex w-full cursor-pointer items-center gap-2 rounded border-none px-3 py-1.5 text-left text-[0.875rem] text-text hover:bg-hover ${
    active ? "bg-hover font-semibold" : "bg-transparent font-normal"
  }`;
}

function apiErrorMessage(err: unknown, fallback: string): string {
  if (!(err instanceof Error)) return fallback;
  const match = err.message.match(/^\d+:\s*([\s\S]*)$/);
  if (!match) return err.message || fallback;
  try {
    const parsed: unknown = JSON.parse(match[1]);
    if (
      typeof parsed === "object" &&
      parsed !== null &&
      "error" in parsed &&
      typeof (parsed as { error: unknown }).error === "string"
    ) {
      return (parsed as { error: string }).error;
    }
  } catch {
    // Body was not JSON; show the raw text.
  }
  return match[1] || fallback;
}

export default function GroupsNav({ groups }: { groups: string[] }) {
  const location = useLocation();
  const navigate = useNavigate();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [renameFor, setRenameFor] = useState<string | null>(null);
  const [menuFor, setMenuFor] = useState<string | null>(null);

  useEffect(() => {
    if (!menuFor) return;
    const onPointerDown = (e: MouseEvent) => {
      const t = e.target;
      if (t instanceof Element && t.closest("[data-group-row-menu]")) return;
      setMenuFor(null);
    };
    document.addEventListener("mousedown", onPointerDown, true);
    return () => document.removeEventListener("mousedown", onPointerDown, true);
  }, [menuFor]);

  const createGroup = async (name: string) => {
    if (isReservedGroupName(name)) {
      setError(reservedGroupError(name));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const created = await createContactGroup(name);
      setCreateOpen(false);
      navigate(`/group/${groupSlug(created)}`);
    } catch (err) {
      setError(apiErrorMessage(err, "Could not create group"));
    } finally {
      setBusy(false);
    }
  };

  const renameGroup = async (from: string, to: string) => {
    if (isReservedGroupName(to)) {
      setError(reservedGroupError(to));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const next = await renameContactGroup(from, to);
      setRenameFor(null);
      if (location.pathname === `/group/${groupSlug(from)}`) {
        navigate(`/group/${groupSlug(next)}`);
      }
    } catch (err) {
      setError(apiErrorMessage(err, "Could not rename group"));
    } finally {
      setBusy(false);
    }
  };

  const removeGroup = async (name: string) => {
    setBusy(true);
    setError(null);
    setMenuFor(null);
    try {
      await deleteContactGroup(name);
      if (location.pathname === `/group/${groupSlug(name)}`) {
        navigate("/contacts");
      }
    } catch (err) {
      setError(apiErrorMessage(err, "Could not delete group"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
    <NavCollapsibleSection
      id="contact-groups"
      title="Contact Groups"
      addLabel="Create group"
      addDisabled={busy}
      onAdd={() => {
        setMenuFor(null);
        setError(null);
        setCreateOpen(true);
      }}
    >
      {groups.map((name) => {
        const href = `/group/${groupSlug(name)}`;
        const active = location.pathname === href;
        const menuOpen = menuFor === name;
        return (
          <div key={name} className="group relative">
            <button
              type="button"
              onClick={() => navigate(href)}
              className={`${navRowClass(active)} pr-8`}
            >
              <PeopleGroupIcon size={15} />
              <span className="min-w-0 flex-1 truncate">{name}</span>
            </button>
            <button
              type="button"
              aria-label={`Group options for ${name}`}
              aria-expanded={menuOpen}
              disabled={busy}
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                setMenuFor(menuOpen ? null : name);
              }}
              className={`absolute top-1/2 right-3 z-10 -translate-y-1/2 cursor-pointer border-none bg-transparent p-0.5 text-muted hover:text-text ${
                active || menuOpen
                  ? "opacity-100"
                  : "pointer-events-none opacity-0 group-hover:pointer-events-auto group-hover:opacity-100"
              }`}
            >
              <EllipsisIcon size={15} />
            </button>
            {menuOpen ? (
              <div
                data-group-row-menu=""
                data-mv-overlay=""
                className="absolute top-full right-0 z-[80] mt-0.5 min-w-[7.5rem] rounded-lg border border-border bg-popover py-1 shadow-xl"
              >
                <button
                  type="button"
                  className="block w-full cursor-pointer border-none bg-transparent px-3 py-1.5 text-left text-[0.813rem] text-text hover:bg-hover"
                  onClick={() => {
                    setMenuFor(null);
                    setError(null);
                    setRenameFor(name);
                  }}
                >
                  Rename…
                </button>
                <button
                  type="button"
                  disabled={busy}
                  className="block w-full cursor-pointer border-none bg-transparent px-3 py-1.5 text-left text-[0.813rem] text-text hover:bg-hover disabled:opacity-40"
                  onClick={() => void removeGroup(name)}
                >
                  Delete
                </button>
              </div>
            ) : null}
          </div>
        );
      })}

      <button
        type="button"
        onClick={() => navigate("/no-group")}
        className={navRowClass(location.pathname === "/no-group")}
      >
        <PersonIcon size={15} />
        <span className="truncate">No group</span>
      </button>
    </NavCollapsibleSection>

      {createOpen ? (
        <GroupNameDialog
          title="Create group"
          error={error}
          busy={busy}
          onSave={createGroup}
          onCancel={() => {
            setCreateOpen(false);
            setError(null);
          }}
        />
      ) : null}
      {renameFor ? (
        <GroupNameDialog
          title="Rename group"
          initial={renameFor}
          error={error}
          busy={busy}
          onSave={(to) => renameGroup(renameFor, to)}
          onCancel={() => {
            setRenameFor(null);
            setError(null);
          }}
        />
      ) : null}
    </>
  );
}
