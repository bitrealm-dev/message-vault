import type { MembershipCheckState } from "../lib/membershipChecks";
import { isReservedTagName, reservedTagError } from "../lib/threadTags";
import GroupsMenu from "./GroupsMenu";
import { TagIcon } from "./icons";

/** Assign or remove thread tags on the selected conversations. */
export default function TagsMenu({
  allTags,
  checks,
  onToggle,
  onCreate,
  onClearAll,
  disabled = false,
}: {
  allTags: string[];
  checks: Record<string, MembershipCheckState>;
  onToggle?: (name: string) => void;
  onCreate?: (name: string) => void;
  onClearAll?: () => void;
  disabled?: boolean;
}) {
  return (
    <GroupsMenu
      allGroups={allTags}
      checks={checks}
      onToggle={onToggle}
      onCreate={onCreate}
      onClearAll={onClearAll}
      disabled={disabled}
      ariaLabel="Tags"
      title="Tags"
      searchPlaceholder="Search tags…"
      emptyText="No tags"
      createButtonLabel="Create tag"
      createTitle="Create thread tag"
      createPlaceholder="Tag name"
      isReserved={isReservedTagName}
      reservedError={reservedTagError}
      icon={<TagIcon size={16} />}
      labeled={false}
    />
  );
}
