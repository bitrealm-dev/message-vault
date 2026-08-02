"use client";

export type ContactEditDraft = {
  preferredName: string;
  phones: string[];
  labels: string[];
};

export function seedContactEditDraft(contact: {
  preferredName?: string | null;
  firstName?: string | null;
  lastName?: string | null;
  phones: string[];
  labels?: string[];
}): ContactEditDraft {
  const preferred =
    contact.preferredName?.trim() ||
    [contact.firstName, contact.lastName].filter(Boolean).join(" ").trim() ||
    "";
  return {
    preferredName: preferred,
    phones: [...contact.phones, ""],
    labels: contact.labels ? [...contact.labels] : [],
  };
}

export function emptyContactEditDraft(defaults?: {
  labels?: string[];
}): ContactEditDraft {
  return {
    preferredName: "",
    phones: [""],
    labels: defaults?.labels ? [...defaults.labels] : [],
  };
}

export function draftHasName(draft: ContactEditDraft): boolean {
  return draft.preferredName.trim() !== "";
}

export function displayLabelNames(labels: string[]): string[] {
  return labels;
}

/** Drop empty non-trailing rows; ensure exactly one trailing empty row. */
export function normalizePhoneRows(phones: string[]): string[] {
  const filled = phones.filter((p, i) => {
    if (i === phones.length - 1) return true;
    return p.trim() !== "";
  });
  const withoutTrailingEmpties = [...filled];
  while (
    withoutTrailingEmpties.length > 1 &&
    withoutTrailingEmpties[withoutTrailingEmpties.length - 1] === "" &&
    withoutTrailingEmpties[withoutTrailingEmpties.length - 2] === ""
  ) {
    withoutTrailingEmpties.pop();
  }
  if (
    withoutTrailingEmpties.length === 0 ||
    withoutTrailingEmpties[withoutTrailingEmpties.length - 1] !== ""
  ) {
    withoutTrailingEmpties.push("");
  }
  return withoutTrailingEmpties;
}

export function updatePhoneAt(
  phones: string[],
  index: number,
  value: string,
): string[] {
  const next = [...phones];
  next[index] = value;
  if (index === phones.length - 1 && value !== "") {
    next.push("");
  }
  return next;
}

export function removePhoneAt(phones: string[], index: number): string[] {
  return normalizePhoneRows(phones.filter((_, i) => i !== index));
}

export function blurPhoneAt(phones: string[], index: number): string[] {
  if (index >= phones.length - 1) return phones;
  if (phones[index]?.trim() !== "") return phones;
  return normalizePhoneRows(phones);
}

/** Phones to persist: non-empty trimmed values, no trailing empty. */
export function phonesForSave(phones: string[]): string[] {
  return phones.map((p) => p.trim()).filter(Boolean);
}

export function ContactPhoneList({
  phones,
  onChange,
  /** When set, refuse to remove/clear below this many non-empty phones. */
  minFilled = 0,
  placeholder = "Phone or email",
  removeLabel = "Remove phone or email",
}: {
  phones: string[];
  onChange: (phones: string[]) => void;
  minFilled?: number;
  placeholder?: string;
  removeLabel?: string;
}) {
  const filledCount = phonesForSave(phones).length;
  return (
    <div className="flex flex-col gap-1.5">
      {phones.map((phone, index) => {
        const showRemove =
          phone.trim() !== "" && filledCount > minFilled;
        return (
          <div key={index} className="flex items-center gap-2">
            <input
              type="text"
              value={phone}
              onChange={(e) => {
                const next = updatePhoneAt(phones, index, e.target.value);
                if (
                  minFilled > 0 &&
                  phonesForSave(next).length < minFilled
                ) {
                  return;
                }
                onChange(next);
              }}
              onBlur={() => onChange(blurPhoneAt(phones, index))}
              placeholder={placeholder}
              className="min-w-0 flex-1 rounded-md border border-border bg-elevated/40 px-2.5 py-1.5 text-[13px] text-text outline-none placeholder:text-muted focus:border-accent/60"
            />
            {showRemove && (
              <button
                type="button"
                onClick={() => onChange(removePhoneAt(phones, index))}
                className="shrink-0 rounded px-1.5 text-[12px] text-muted hover:bg-hover hover:text-text"
                aria-label={removeLabel}
              >
                ×
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}
