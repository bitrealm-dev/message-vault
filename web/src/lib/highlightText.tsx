import type { ReactNode } from "react";

/** Wrap every case-insensitive occurrence of `term` in a <mark>. */
export function highlightText(text: string, term: string): ReactNode[] {
  const t = term.trim().toLowerCase();
  if (!t) return [text];
  const out: ReactNode[] = [];
  let rest = text;
  let key = 0;
  while (true) {
    const idx = rest.toLowerCase().indexOf(t);
    if (idx === -1) {
      out.push(rest);
      break;
    }
    if (idx > 0) out.push(rest.slice(0, idx));
    out.push(
      <mark key={key++} className="rounded-sm bg-search-mark px-px">
        {rest.slice(idx, idx + t.length)}
      </mark>,
    );
    rest = rest.slice(idx + t.length);
  }
  return out;
}
