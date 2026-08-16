import { useEffect, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { useAuth } from "../lib/auth";
import { shouldIgnoreOutsideDismiss } from "../lib/portaledOverlay";
import { popupShadow } from "../lib/uiStyles";
import { ChevronRightIcon, GearIcon, SignOutIcon } from "./icons";

const itemClass =
  "flex w-full cursor-pointer items-center gap-2 border-none bg-transparent px-3 py-1.5 text-left text-[0.813rem] text-text hover:bg-hover";

/** App name in the header; opens Settings and Sign out. */
export default function AppAccountMenu() {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const navigate = useNavigate();
  const location = useLocation();
  const { logout } = useAuth();
  const settingsActive = location.pathname.startsWith("/settings");

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (e: MouseEvent) => {
      if (shouldIgnoreOutsideDismiss(e, rootRef.current)) return;
      setOpen(false);
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        aria-expanded={open}
        aria-haspopup="menu"
        aria-label="Account menu"
        onClick={() => setOpen((v) => !v)}
        className="flex cursor-pointer items-center gap-1 border-none bg-transparent p-0 text-[0.875rem] font-bold text-text"
      >
        Message Vault
        <ChevronRightIcon
          size={12}
          className={`shrink-0 text-muted transition-transform duration-150 ${
            open ? "rotate-90" : ""
          }`}
        />
      </button>
      {open ? (
        <div
          role="menu"
          data-mv-overlay=""
          className={`absolute left-0 top-full z-[100] mt-1 min-w-[11rem] rounded-xl border border-border bg-popover py-1 ${popupShadow}`}
        >
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              navigate("/settings");
            }}
            className={`${itemClass} ${settingsActive ? "bg-hover font-semibold" : ""}`}
          >
            <GearIcon size={15} />
            Settings
          </button>
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              logout();
            }}
            className={`${itemClass} text-danger`}
          >
            <SignOutIcon size={15} />
            Sign out
          </button>
        </div>
      ) : null}
    </div>
  );
}
