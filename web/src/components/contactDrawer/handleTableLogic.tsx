import type { ReactNode } from "react";
import type { ContactHandle } from "../../lib/contactDetail";
import { formatHandleDate, formatHandleServiceLabel } from "./contactDrawerTypes";

export function handleDateCell(iso: string | null | undefined): string {
  return formatHandleDate(iso) ?? "—";
}

export function conversationCount(h: {
  individual_conversations: number;
  group_conversations: number;
}): number {
  return h.individual_conversations + h.group_conversations;
}

export type RemoveIdentityTarget = {
  handle: string;
  /** Storage id (`phone` | `whatsapp`) for the mutation API. */
  service: string | null;
  serviceLabel: string;
  threadCount: number;
};

export function removeIdentityConfirmBody(target: RemoveIdentityTarget): ReactNode {
  const { handle, serviceLabel, threadCount } = target;
  const emphasize = "font-medium text-accent";
  const serviceId = (
    <>
      <span className={emphasize}>{serviceLabel}</span>{" "}
      <span className={`${emphasize} break-all`}>{handle}</span>
    </>
  );
  if (threadCount <= 0) {
    return (
      <p className="mt-3 text-[0.875rem] leading-relaxed text-muted">
        Removing {serviceId} will unlink it from this contact.
      </p>
    );
  }
  const threadWord = threadCount === 1 ? "thread" : "threads";
  return (
    <p className="mt-3 text-[0.875rem] leading-relaxed text-muted">
      Removing {serviceId} will unlink {threadCount} {threadWord} from this contact. Unlinked data
      will not be deleted.
    </p>
  );
}

export function sortValue(h: ContactHandle, col: string): string | number {
  switch (col) {
    case "service":
      return formatHandleServiceLabel(h.handle, h.service).toLowerCase();
    case "handle":
      return h.handle.toLowerCase();
    case "start_date":
      return h.start_date ?? "";
    case "end_date":
      return h.end_date ?? "";
    case "conversations":
      return conversationCount(h);
    case "direct_messages":
      return h.individual_message_count;
    case "group_messages":
      return h.group_message_count;
    default:
      return "";
  }
}
