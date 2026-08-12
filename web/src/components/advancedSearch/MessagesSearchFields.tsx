import Select, { ListBoxItem as SelectListBoxItem } from "../Select";
import { parseSelectKey } from "../../lib/selectKey";
import type { CountFilterInput } from "./buildAdvancedQuery";
import CountField from "./CountField";
import {
  compactSelectItemClassName,
  inputClass,
  labelClass,
  selectTriggerClass,
} from "./advancedSearchStyles";

export default function MessagesSearchFields({
  nameOrHandle,
  onNameOrHandleChange,
  handle,
  onHandleChange,
  msgType,
  onMsgTypeChange,
  participants,
  onParticipantsChange,
}: {
  nameOrHandle: string;
  onNameOrHandleChange: (value: string) => void;
  handle: string;
  onHandleChange: (value: string) => void;
  msgType: "all" | "direct" | "group";
  onMsgTypeChange: (value: "all" | "direct" | "group") => void;
  participants: CountFilterInput;
  onParticipantsChange: (value: CountFilterInput) => void;
}) {
  return (
    <div className="grid grid-cols-2 gap-3">
      <div className="col-span-2">
        <label className={labelClass}>Name or title</label>
        <input
          className={inputClass}
          value={nameOrHandle}
          onChange={(e) => onNameOrHandleChange(e.target.value)}
          placeholder="Gregory Coleman"
        />
      </div>
      <div>
        <label className={labelClass}>Identity</label>
        <input
          className={inputClass}
          value={handle}
          onChange={(e) => onHandleChange(e.target.value)}
          placeholder="+15555550100"
        />
      </div>
      <div>
        <label className={labelClass}>Conversation type</label>
        <Select
          selectedKey={msgType}
          onSelectionChange={(k) => {
            const next = parseSelectKey(k, ["all", "direct", "group"] as const);
            if (next) onMsgTypeChange(next);
          }}
          aria-label="Conversation type"
          triggerClassName={selectTriggerClass}
        >
          <SelectListBoxItem id="all" className={compactSelectItemClassName}>All</SelectListBoxItem>
          <SelectListBoxItem id="direct" className={compactSelectItemClassName}>Direct</SelectListBoxItem>
          <SelectListBoxItem id="group" className={compactSelectItemClassName}>Group</SelectListBoxItem>
        </Select>
      </div>
      <CountField
        label="Group participants"
        value={participants}
        onChange={onParticipantsChange}
      />
    </div>
  );
}
