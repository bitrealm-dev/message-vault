import { useState } from "react";
import { Button, Tag, TagGroup, TagList } from "react-aria-components";
import { commitPhoneTokens, removePhoneToken } from "../lib/phoneTokens";
import { textInputClassName } from "./TextField";

export type PhoneTokenFieldProps = {
  value: string[];
  onChange: (phones: string[]) => void;
  "aria-label"?: string;
  placeholder?: string;
};

/**
 * Multi-value phone field: each number is a removable token.
 * Enter, comma, or blur commits the draft; Backspace on empty draft removes the last token.
 */
export default function PhoneTokenField({
  value,
  onChange,
  "aria-label": ariaLabel = "Backup Device Phone Number",
  placeholder = "+1 555-123-4567",
}: PhoneTokenFieldProps) {
  const [draft, setDraft] = useState("");

  function commitDraft(): void {
    const trimmed = draft.trim();
    if (!trimmed) {
      setDraft("");
      return;
    }
    onChange(commitPhoneTokens(value, trimmed));
    setDraft("");
  }

  return (
    <div
      className={`${textInputClassName} flex min-h-[2.75rem] flex-wrap items-center gap-1.5 py-1.5 focus-within:border-accent`}
    >
      <TagGroup
        aria-label={ariaLabel}
        onRemove={(keys) => {
          const key = [...keys][0];
          if (typeof key === "string") onChange(removePhoneToken(value, key));
        }}
      >
        <TagList className="contents">
          {value.map((phone) => (
            <Tag
              key={phone}
              id={phone}
              textValue={phone}
              className="inline-flex items-center gap-1 rounded-lg border border-border bg-panel px-2 py-0.5 text-[0.8125rem] text-text outline-none data-[selected]:border-accent"
            >
              {phone}
              <Button
                slot="remove"
                className="cursor-pointer border-0 bg-transparent p-0 text-muted hover:text-text"
                aria-label={`Remove ${phone}`}
              >
                ×
              </Button>
            </Tag>
          ))}
        </TagList>
      </TagGroup>
      <input
        type="text"
        value={draft}
        aria-label={ariaLabel}
        placeholder={value.length === 0 ? placeholder : "Add another"}
        className="min-w-[8rem] flex-1 border-0 bg-transparent p-0 text-[0.875rem] text-text outline-none"
        onChange={(e) => {
          const next = e.target.value;
          if (next.includes(",")) {
            onChange(commitPhoneTokens(value, next));
            setDraft("");
            return;
          }
          setDraft(next);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            commitDraft();
            return;
          }
          if (e.key === "Backspace" && draft.length === 0 && value.length > 0) {
            e.preventDefault();
            onChange(value.slice(0, -1));
          }
        }}
        onBlur={commitDraft}
      />
    </div>
  );
}
