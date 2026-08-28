import { contactGroups, groupSlug } from "../lib/contactGroups";
import { PeopleGroupIcon, PersonIcon } from "./icons";
import NavEntityList, { type NavEntityCopy } from "./NavEntityList";

const COPY: NavEntityCopy = {
  id: "contact-groups",
  title: "Contact Groups",
  routeBase: "/group",
  emptyRoute: "/no-group",
  emptyLabel: "No group",
  fallbackRoute: "/contacts",
  addLabel: "Create contact group",
  createTitle: "Create contact group",
  renameTitle: "Rename group",
  namePlaceholder: "Group name",
  optionsLabel: (name) => `Group options for ${name}`,
  createError: "Could not create group",
  renameError: "Could not rename group",
  deleteError: "Could not delete group",
};

export default function GroupsNav({ groups }: { groups: string[] }) {
  return (
    <NavEntityList
      names={groups}
      collection={contactGroups}
      slug={groupSlug}
      icon={<PeopleGroupIcon size={15} />}
      emptyIcon={<PersonIcon size={15} />}
      copy={COPY}
    />
  );
}
