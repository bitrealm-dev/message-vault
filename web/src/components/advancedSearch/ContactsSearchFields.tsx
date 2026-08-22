import { useId } from "react";
import type { Key } from "react-aria-components";
import { parseSelectKey } from "../../lib/selectKey";
import Select, { ListBoxItem as SelectListBoxItem } from "../Select";
import {
  compactFieldTriggerClass,
  compactSelectItemClassName,
  contactStackClass,
  inputClass,
  labelClass,
} from "./advancedSearchStyles";
import type { ActivityFilter, DateBoundFilter } from "./buildAdvancedQuery";
import { EMPTY_DATE_BOUND } from "./buildAdvancedQuery";
import DateBoundField from "./DateBoundField";
import ServiceMultiSelect from "./ServiceMultiSelect";

export default function ContactsSearchFields({
  contactName,
  onContactNameChange,
  contactNameSaved,
  onContactNameSavedChange,
  handle,
  onHandleChange,
  handleSaved,
  onHandleSavedChange,
  noPreferredName,
  onNoPreferredNameChange,
  noHandle,
  onNoHandleChange,
  services,
  onServicesChange,
  firstMsgBound,
  onFirstMsgBoundChange,
  lastMsgBound,
  onLastMsgBoundChange,
  activity,
  onActivityChange,
  lockedByNoHandle,
  onLockedByNoHandleChange,
}: {
  contactName: string;
  onContactNameChange: (value: string) => void;
  contactNameSaved: string;
  onContactNameSavedChange: (value: string) => void;
  handle: string;
  onHandleChange: (value: string) => void;
  handleSaved: string;
  onHandleSavedChange: (value: string) => void;
  noPreferredName: boolean;
  onNoPreferredNameChange: (value: boolean) => void;
  noHandle: boolean;
  onNoHandleChange: (value: boolean) => void;
  services: Key[];
  onServicesChange: (value: Key[]) => void;
  firstMsgBound: DateBoundFilter;
  onFirstMsgBoundChange: (value: DateBoundFilter) => void;
  lastMsgBound: DateBoundFilter;
  onLastMsgBoundChange: (value: DateBoundFilter) => void;
  activity: ActivityFilter;
  onActivityChange: (value: ActivityFilter) => void;
  lockedByNoHandle: {
    services: Key[];
    firstMsgBound: DateBoundFilter;
    lastMsgBound: DateBoundFilter;
    activity: ActivityFilter;
  } | null;
  onLockedByNoHandleChange: (
    value: {
      services: Key[];
      firstMsgBound: DateBoundFilter;
      lastMsgBound: DateBoundFilter;
      activity: ActivityFilter;
    } | null,
  ) => void;
}) {
  const activityId = useId();

  return (
    <div className={contactStackClass}>
      <div className="min-w-0">
        <label className="block">
          <span className={labelClass}>Name</span>
          <input
            className={`${inputClass} ${noPreferredName ? "cursor-not-allowed opacity-40" : ""}`}
            value={contactName}
            disabled={noPreferredName}
            onChange={(e) => onContactNameChange(e.target.value)}
            placeholder={noPreferredName ? undefined : "Pat Lee"}
          />
        </label>
        <label className="mt-2 inline-flex cursor-pointer items-center gap-2 text-[0.813rem] text-text">
          <input
            type="checkbox"
            checked={noPreferredName}
            onChange={(e) => {
              const checked = e.target.checked;
              if (checked) {
                onContactNameSavedChange(contactName);
                onContactNameChange("");
                onNoPreferredNameChange(true);
              } else {
                onContactNameChange(contactNameSaved);
                onContactNameSavedChange("");
                onNoPreferredNameChange(false);
              }
            }}
            className="checkbox-list"
          />
          No name
        </label>
      </div>
      <div className="min-w-0">
        <label className="block">
          <span className={labelClass}>Identity</span>
          <input
            className={`${inputClass} ${noHandle ? "cursor-not-allowed opacity-40" : ""}`}
            value={handle}
            disabled={noHandle}
            onChange={(e) => onHandleChange(e.target.value)}
            placeholder={noHandle ? undefined : "+15555550100"}
          />
        </label>
        <label className="mt-2 inline-flex cursor-pointer items-center gap-2 text-[0.813rem] text-text">
          <input
            type="checkbox"
            checked={noHandle}
            onChange={(e) => {
              const checked = e.target.checked;
              if (checked) {
                onHandleSavedChange(handle);
                onHandleChange("");
                onLockedByNoHandleChange({
                  services,
                  firstMsgBound,
                  lastMsgBound,
                  activity,
                });
                onServicesChange([]);
                onFirstMsgBoundChange(EMPTY_DATE_BOUND);
                onLastMsgBoundChange(EMPTY_DATE_BOUND);
                onActivityChange("any");
                onNoHandleChange(true);
              } else {
                onHandleChange(handleSaved);
                onHandleSavedChange("");
                if (lockedByNoHandle) {
                  onServicesChange(lockedByNoHandle.services);
                  onFirstMsgBoundChange(lockedByNoHandle.firstMsgBound);
                  onLastMsgBoundChange(lockedByNoHandle.lastMsgBound);
                  onActivityChange(lockedByNoHandle.activity);
                  onLockedByNoHandleChange(null);
                }
                onNoHandleChange(false);
              }
            }}
            className="checkbox-list"
          />
          No identity
        </label>
      </div>
      <ServiceMultiSelect value={services} onChange={onServicesChange} isDisabled={noHandle} />
      <DateBoundField
        label="First Seen"
        value={firstMsgBound}
        onChange={onFirstMsgBoundChange}
        isDisabled={noHandle}
      />
      <DateBoundField
        label="Last Seen"
        value={lastMsgBound}
        onChange={onLastMsgBoundChange}
        isDisabled={noHandle}
      />
      <div className={`min-w-0 ${noHandle ? "opacity-40" : ""}`}>
        <label htmlFor={activityId} className={labelClass}>
          Activity
        </label>
        <Select
          id={activityId}
          selectedKey={activity}
          onSelectionChange={(k) => {
            const next = parseSelectKey(k, ["any", "messages", "no-messages"] as const);
            if (next) onActivityChange(next);
          }}
          aria-label="Activity"
          className="w-full min-w-0"
          triggerClassName={compactFieldTriggerClass}
          isDisabled={noHandle}
        >
          <SelectListBoxItem id="any" className={compactSelectItemClassName}>
            Any
          </SelectListBoxItem>
          <SelectListBoxItem id="messages" className={compactSelectItemClassName}>
            Has messages
          </SelectListBoxItem>
          <SelectListBoxItem id="no-messages" className={compactSelectItemClassName}>
            Never messaged
          </SelectListBoxItem>
        </Select>
      </div>
    </div>
  );
}
