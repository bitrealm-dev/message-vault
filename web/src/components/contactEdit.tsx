"use client";

export type ContactEditDraft = {
  firstName: string;
  lastName: string;
  phones: string[];
  labels: string[];
};

export function seedContactEditDraft(contact: {
  firstName: string | null;
  lastName: string | null;
  phones: string[];
  labels?: string[];
}): ContactEditDraft {
  return {
    firstName: contact.firstName ?? "",
    lastName: contact.lastName ?? "",
    phones: [...contact.phones, ""],
    labels: contact.labels ? [...contact.labels] : [],
  };
}

export function emptyContactEditDraft(defaults?: {
  labels?: string[];
}): ContactEditDraft {
  return {
    firstName: "",
    lastName: "",
    phones: [""],
    labels: defaults?.labels ? [...defaults.labels] : [],
  };
}

export function draftHasName(draft: ContactEditDraft): boolean {
  return draft.firstName.trim() !== "" || draft.lastName.trim() !== "";
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
}: {
  phones: string[];
  onChange: (phones: string[]) => void;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      {phones.map((phone, index) => {
        const showRemove = phone.trim() !== "";
        return (
          <div key={index} className="flex items-center gap-2">
            <input
              type="text"
              value={phone}
              onChange={(e) =>
                onChange(updatePhoneAt(phones, index, e.target.value))
              }
              onBlur={() => onChange(blurPhoneAt(phones, index))}
              placeholder="Phone or email"
              className="min-w-0 flex-1 rounded-md border border-border bg-transparent px-2 py-1 text-[13px] text-text outline-none placeholder:text-muted focus:border-accent/60"
            />
            {showRemove && (
              <button
                type="button"
                onClick={() => onChange(removePhoneAt(phones, index))}
                className="shrink-0 rounded px-1.5 text-[12px] text-muted hover:bg-hover hover:text-text"
                aria-label="Remove handle"
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
