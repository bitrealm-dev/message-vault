import ContactInitialCircle from "./ContactInitialCircle";
import { listRowDividersThin } from "../lib/tw";
import type { ContactPreview } from "./ContactDrawer";

/** Right-hand list of contacts whose checkboxes are on. */
export default function CheckedContactsPanel({
  contacts,
}: {
  contacts: ContactPreview[];
}) {
  const heading =
    contacts.length === 1
      ? "1 contact selected"
      : `${contacts.length} contacts selected`;

  return (
    <aside
      className="flex min-h-0 min-w-0 flex-1 flex-col overflow-auto border-l border-border bg-panel outline-none"
      aria-label={heading}
    >
      <div className="border-b border-border px-6 py-4">
        <h2 className="m-0 text-[1.125rem] font-semibold">{heading}</h2>
      </div>
      <ul className="m-0 list-none p-0">
        {contacts.map((contact) => (
          <li
            key={contact.id}
            className={`flex items-center gap-2.5 px-6 py-2 ${listRowDividersThin}`}
          >
            <ContactInitialCircle
              displayName={contact.name}
              preferredHandle={contact.handles?.[0] ?? null}
            />
            <span className="min-w-0 truncate text-[0.875rem] font-medium">
              {contact.name}
            </span>
          </li>
        ))}
      </ul>
    </aside>
  );
}
