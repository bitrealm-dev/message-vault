import { type ReactNode, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { apiErrorMessage } from "../lib/apiErrorMessage";
import type { NameCollection } from "../lib/nameCollection";
import GroupNameDialog from "./GroupNameDialog";
import { EllipsisIcon } from "./icons";
import NavCollapsibleSection from "./NavCollapsibleSection";
import NavGlyphButton from "./NavGlyphButton";
import {
  NAV_LEADING_GLYPH_CLASS,
  NAV_NESTED_ROW_CLASS,
  navGlyphRowClass,
} from "./navSectionLayout";
import PopupMenu from "./PopupMenu";

/**
 * The sidebar section listing one named collection: contact groups or message
 * tags. Both were 248-line components differing only in vocabulary, down to a
 * byte-identical copy of `apiErrorMessage`.
 */
export type NavEntityCopy = {
  /** Section id used for the collapse state. */
  id: string;
  /** Section heading, e.g. "Contact Groups". */
  title: string;
  /** Route prefix for one entity, e.g. "/group". */
  routeBase: string;
  /** Route for the "none of these" page, e.g. "/no-group". */
  emptyRoute: string;
  /** Label of the "none of these" row, e.g. "No group". */
  emptyLabel: string;
  /** Where a delete sends the user when they were on the deleted page. */
  fallbackRoute: string;
  addLabel: string;
  createTitle: string;
  renameTitle: string;
  namePlaceholder: string;
  /** Menu button label, completed with the entity name. */
  optionsLabel: (name: string) => string;
  createError: string;
  renameError: string;
  deleteError: string;
};

export default function NavEntityList({
  names,
  collection,
  slug,
  icon,
  emptyIcon,
  copy,
}: {
  names: string[];
  collection: NameCollection;
  slug: (name: string) => string;
  icon: ReactNode;
  emptyIcon: ReactNode;
  copy: NavEntityCopy;
}) {
  const location = useLocation();
  const navigate = useNavigate();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [renameFor, setRenameFor] = useState<string | null>(null);
  const [menuFor, setMenuFor] = useState<string | null>(null);
  const menuTriggerRef = useRef<HTMLButtonElement>(null);

  const create = async (name: string) => {
    if (collection.isReserved(name)) {
      setError(collection.reservedError(name));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const created = await collection.create(name);
      setCreateOpen(false);
      navigate(`${copy.routeBase}/${slug(created)}`);
    } catch (err) {
      setError(apiErrorMessage(err, copy.createError));
    } finally {
      setBusy(false);
    }
  };

  const rename = async (from: string, to: string) => {
    if (collection.isReserved(to)) {
      setError(collection.reservedError(to));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const next = await collection.rename(from, to);
      setRenameFor(null);
      if (location.pathname === `${copy.routeBase}/${slug(from)}`) {
        navigate(`${copy.routeBase}/${slug(next)}`);
      }
    } catch (err) {
      setError(apiErrorMessage(err, copy.renameError));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (name: string) => {
    setBusy(true);
    setError(null);
    setMenuFor(null);
    try {
      await collection.remove(name);
      if (location.pathname === `${copy.routeBase}/${slug(name)}`) {
        navigate(copy.fallbackRoute);
      }
    } catch (err) {
      setError(apiErrorMessage(err, copy.deleteError));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <NavCollapsibleSection
        id={copy.id}
        title={copy.title}
        addLabel={copy.addLabel}
        addDisabled={busy}
        onAdd={() => {
          setMenuFor(null);
          setError(null);
          setCreateOpen(true);
        }}
      >
        {names.map((name) => {
          const href = `${copy.routeBase}/${slug(name)}`;
          const active = location.pathname === href;
          const menuOpen = menuFor === name;
          return (
            <div key={name} className="relative w-full">
              <div className={navGlyphRowClass(active)}>
                <button
                  type="button"
                  onClick={() => navigate(href)}
                  className={`${NAV_NESTED_ROW_CLASS} cursor-pointer border-none bg-transparent p-0 text-left text-inherit`}
                >
                  <span className={NAV_LEADING_GLYPH_CLASS}>{icon}</span>
                  <span className="min-w-0 truncate">{name}</span>
                </button>
                <NavGlyphButton
                  aria-label={copy.optionsLabel(name)}
                  aria-haspopup="menu"
                  aria-expanded={menuOpen}
                  disabled={busy}
                  active={menuOpen}
                  onClick={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    // Captured here rather than through a conditional `ref`,
                    // which would detach before the menu's close effect runs
                    // and leave nothing to return focus to.
                    menuTriggerRef.current = e.currentTarget;
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
              <PopupMenu
                open={menuOpen}
                onClose={() => setMenuFor(null)}
                triggerRef={menuTriggerRef}
                label={copy.optionsLabel(name)}
                className="absolute top-full right-0 z-[80] mt-0.5"
                items={[
                  {
                    label: "Rename…",
                    onSelect: () => {
                      setError(null);
                      setRenameFor(name);
                    },
                  },
                  {
                    label: "Delete",
                    disabled: busy,
                    onSelect: () => void remove(name),
                  },
                ]}
              />
            </div>
          );
        })}

        <button
          type="button"
          onClick={() => navigate(copy.emptyRoute)}
          className={`${navGlyphRowClass(location.pathname === copy.emptyRoute)} cursor-pointer`}
        >
          <span className={NAV_NESTED_ROW_CLASS}>
            <span className={NAV_LEADING_GLYPH_CLASS}>{emptyIcon}</span>
            <span className="truncate">{copy.emptyLabel}</span>
          </span>
        </button>
      </NavCollapsibleSection>

      {createOpen ? (
        <GroupNameDialog
          title={copy.createTitle}
          placeholder={copy.namePlaceholder}
          confirmLabel="Create"
          error={error}
          busy={busy}
          onSave={create}
          onCancel={() => {
            setCreateOpen(false);
            setError(null);
          }}
        />
      ) : null}
      {renameFor ? (
        <GroupNameDialog
          title={copy.renameTitle}
          placeholder={copy.namePlaceholder}
          initial={renameFor}
          error={error}
          busy={busy}
          onSave={(to) => rename(renameFor, to)}
          onCancel={() => {
            setRenameFor(null);
            setError(null);
          }}
        />
      ) : null}
    </>
  );
}
