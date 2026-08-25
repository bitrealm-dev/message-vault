import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import {
  createThreadTag,
  deleteThreadTag,
  isReservedTagName,
  renameThreadTag,
  reservedTagError,
  tagSlug,
} from "../lib/threadTags";
import GroupNameDialog from "./GroupNameDialog";
import { EllipsisIcon, TagIcon } from "./icons";
import NavCollapsibleSection from "./NavCollapsibleSection";
import NavGlyphButton from "./NavGlyphButton";
import { navGlyphRowClass } from "./navSectionLayout";

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

export default function ThreadTagsNav({ tags }: { tags: string[] }) {
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
      if (t instanceof Element && t.closest("[data-tag-row-menu]")) return;
      setMenuFor(null);
    };
    document.addEventListener("mousedown", onPointerDown, true);
    return () => document.removeEventListener("mousedown", onPointerDown, true);
  }, [menuFor]);

  const createTag = async (name: string) => {
    if (isReservedTagName(name)) {
      setError(reservedTagError(name));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const created = await createThreadTag(name);
      setCreateOpen(false);
      navigate(`/tag/${tagSlug(created)}`);
    } catch (err) {
      setError(apiErrorMessage(err, "Could not create tag"));
    } finally {
      setBusy(false);
    }
  };

  const renameTag = async (from: string, to: string) => {
    if (isReservedTagName(to)) {
      setError(reservedTagError(to));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const next = await renameThreadTag(from, to);
      setRenameFor(null);
      if (location.pathname === `/tag/${tagSlug(from)}`) {
        navigate(`/tag/${tagSlug(next)}`);
      }
    } catch (err) {
      setError(apiErrorMessage(err, "Could not rename tag"));
    } finally {
      setBusy(false);
    }
  };

  const removeTag = async (name: string) => {
    setBusy(true);
    setError(null);
    setMenuFor(null);
    try {
      await deleteThreadTag(name);
      if (location.pathname === `/tag/${tagSlug(name)}`) {
        navigate("/");
      }
    } catch (err) {
      setError(apiErrorMessage(err, "Could not delete tag"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <NavCollapsibleSection
        id="thread-tags"
        title="Thread Tags"
        addLabel="Create thread tag"
        addDisabled={busy}
        onAdd={() => {
          setMenuFor(null);
          setError(null);
          setCreateOpen(true);
        }}
      >
        {tags.map((name) => {
          const href = `/tag/${tagSlug(name)}`;
          const active = location.pathname === href;
          const menuOpen = menuFor === name;
          return (
            <div key={name} className="relative w-full">
              <div className={navGlyphRowClass(active)}>
                <button
                  type="button"
                  onClick={() => navigate(href)}
                  className="flex min-w-0 cursor-pointer items-center gap-2 border-none bg-transparent p-0 pl-3 text-left text-inherit"
                >
                  <TagIcon size={15} />
                  <span className="min-w-0 truncate">{name}</span>
                </button>
                <NavGlyphButton
                  data-tag-row-menu=""
                  aria-label={`Tag options for ${name}`}
                  aria-expanded={menuOpen}
                  disabled={busy}
                  active={menuOpen}
                  onClick={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    setMenuFor(menuOpen ? null : name);
                  }}
                  className={
                    active || menuOpen
                      ? ""
                      : "opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                  }
                >
                  <EllipsisIcon size={15} />
                </NavGlyphButton>
              </div>
              {menuOpen ? (
                <div
                  data-tag-row-menu=""
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
                    onClick={() => void removeTag(name)}
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
          onClick={() => navigate("/no-tag")}
          className={`${navGlyphRowClass(location.pathname === "/no-tag")} cursor-pointer bg-transparent`}
        >
          <span className="flex min-w-0 items-center gap-2 pl-3">
            <TagIcon size={15} />
            <span className="truncate">No tag</span>
          </span>
        </button>
      </NavCollapsibleSection>

      {createOpen ? (
        <GroupNameDialog
          title="Create thread tag"
          placeholder="Tag name"
          confirmLabel="Create"
          error={error}
          busy={busy}
          onSave={createTag}
          onCancel={() => {
            setCreateOpen(false);
            setError(null);
          }}
        />
      ) : null}
      {renameFor ? (
        <GroupNameDialog
          title="Rename tag"
          placeholder="Tag name"
          initial={renameFor}
          error={error}
          busy={busy}
          onSave={(to) => renameTag(renameFor, to)}
          onCancel={() => {
            setRenameFor(null);
            setError(null);
          }}
        />
      ) : null}
    </>
  );
}
