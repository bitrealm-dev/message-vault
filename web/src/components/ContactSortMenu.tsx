import type { ContactNameSort, ContactNameSortState, ContactSortOrder } from "../lib/contactSort";
import SortMenu, { type SortField } from "./SortMenu";

const FIELDS: ReadonlyArray<SortField<ContactNameSort>> = [
  { id: "first", label: "First Name" },
  { id: "last", label: "Last Name" },
];

export default function ContactSortMenu({
  sort,
  order,
  onChange,
}: {
  sort: ContactNameSort;
  order: ContactSortOrder;
  onChange: (next: ContactNameSortState) => void;
}) {
  return (
    <SortMenu fields={FIELDS} sort={sort} order={order} onChange={onChange} itemNoun="contacts" />
  );
}
