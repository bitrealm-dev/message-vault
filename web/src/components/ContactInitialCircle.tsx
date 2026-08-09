import type { CSSProperties } from "react";
import { contactAvatarColor, contactInitials } from "../lib/contactInitials";

/** Colored circle with contact initials (matches web-next BrowseContactRow). */
export default function ContactInitialCircle({
  displayName,
  preferredHandle,
  size = 28,
}: {
  displayName: string;
  preferredHandle?: string | null;
  size?: number;
}) {
  const avatarInput = {
    displayName,
    preferredName: displayName,
    preferredHandle: preferredHandle ?? null,
  };

  const style: CSSProperties = {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: size,
    height: size,
    flexShrink: 0,
    borderRadius: "999px",
    backgroundColor: contactAvatarColor(avatarInput),
    color: "#ffffff",
    fontSize: size <= 28 ? "11px" : "13px",
    fontWeight: 600,
    lineHeight: 1,
    userSelect: "none",
  };

  return (
    <span aria-hidden style={style}>
      {contactInitials(avatarInput)}
    </span>
  );
}
