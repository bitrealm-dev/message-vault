import { useCallback, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { useAuth } from "../lib/auth";
import { Z_POPOVER } from "../lib/zLayers";
import { ChevronRightIcon, GearIcon, SignOutIcon } from "./icons";
import PopupMenu from "./PopupMenu";

const itemRow = "flex items-center gap-2";

/** App name in the header; opens Settings and Sign out. */
export default function AppAccountMenu() {
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const navigate = useNavigate();
  const location = useLocation();
  const { logout } = useAuth();
  const settingsActive = location.pathname.startsWith("/settings");

  const close = useCallback(() => setOpen(false), []);

  return (
    <div className="relative">
      <button
        type="button"
        ref={triggerRef}
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
      <PopupMenu
        open={open}
        onClose={close}
        triggerRef={triggerRef}
        label="Account menu"
        className={`absolute top-full left-0 mt-1 min-w-[11rem] rounded-xl ${Z_POPOVER}`}
        items={[
          {
            label: "Settings",
            onSelect: () => navigate("/settings"),
            children: (
              <span className={`${itemRow} ${settingsActive ? "font-semibold" : ""}`}>
                <GearIcon size={15} />
                Settings
              </span>
            ),
          },
          {
            label: "Sign out",
            danger: true,
            onSelect: () => logout(),
            children: (
              <span className={itemRow}>
                <SignOutIcon size={15} />
                Sign out
              </span>
            ),
          },
        ]}
      />
    </div>
  );
}
