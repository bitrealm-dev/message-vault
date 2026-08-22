import { parseSelectKey } from "../../lib/selectKey";
import Select, { ListBoxItem as SelectListBoxItem } from "../Select";
import {
  compactSelectItemClassName,
  inputClass,
  labelClass,
  selectTriggerClass,
} from "./advancedSearchStyles";
import type { CountFilterInput } from "./buildAdvancedQuery";

export default function CountField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: CountFilterInput;
  onChange: (next: CountFilterInput) => void;
}) {
  return (
    <div>
      <label className={labelClass}>{label}</label>
      <div className="grid grid-cols-[7rem_minmax(0,1fr)] gap-1.5">
        <Select
          selectedKey={value.comparator}
          aria-label={`${label} comparison`}
          triggerClassName={selectTriggerClass}
          onSelectionChange={(k) => {
            const comparator = parseSelectKey(k, ["any", "=", ">", "<"] as const);
            if (!comparator) return;
            onChange({
              comparator,
              value: comparator === "any" ? "" : value.value,
            });
          }}
        >
          <SelectListBoxItem id="any" className={compactSelectItemClassName}>
            Any
          </SelectListBoxItem>
          <SelectListBoxItem id="=" className={compactSelectItemClassName}>
            Equal to
          </SelectListBoxItem>
          <SelectListBoxItem id=">" className={compactSelectItemClassName}>
            More than
          </SelectListBoxItem>
          <SelectListBoxItem id="<" className={compactSelectItemClassName}>
            Less than
          </SelectListBoxItem>
        </Select>
        <input
          type="number"
          min={0}
          step={1}
          className={`${inputClass} ${value.comparator === "any" ? "opacity-40" : ""}`}
          value={value.comparator === "any" ? "" : value.value}
          disabled={value.comparator === "any"}
          aria-label={`${label} value`}
          onChange={(e) => onChange({ ...value, value: e.target.value })}
        />
      </div>
    </div>
  );
}
