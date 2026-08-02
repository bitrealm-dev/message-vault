import Database from "better-sqlite3";
import { currentAccountId } from "./accountScope";
import { createContact, patchContact } from "./contactsWrite";
import { getContact } from "./contactsRead";
import { dbPath } from "./paths";
import { toPhoneE164 } from "./phoneE164";
import { assertVaultWritable } from "./owner";
import { isReservedLabelName, reservedLabelError } from "./reservedLabels";
import { cardToDraft, parseVcfText } from "./vcfParse";

export type VcfCategoryMapping = {
  /** Category name as found in the VCF. */
  source: string;
  /** Destination vault label name (defaults to source). */
  target: string;
  /** When false, this category is not copied into vault labels. */
  enabled: boolean;
};

export type VcfCategoryPreview = {
  source: string;
  /** Matched contacts that carry this category. */
  matchedCount: number;
};

export type VcfImportPreview = {
  cardsTotal: number;
  matched: number;
  unmatched: number;
  skippedNoPhone: number;
  categories: VcfCategoryPreview[];
};

export type VcfImportSummary = {
  cardsTotal: number;
  matched: number;
  unmatched: number;
  created: number;
  updated: number;
  skipped: number;
  errors: string[];
};

function normalizePhones(raw: string[]): string[] {
  const out: string[] = [];
  for (const p of raw) {
    const e164 = toPhoneE164(p);
    if (!e164) continue;
    if (!out.includes(e164)) out.push(e164);
  }
  return out;
}

/** Phone handles that appear on conversations with messages for this account. */
export function messagePhoneHandles(accountId: string): Set<string> {
  const db = new Database(dbPath(), { readonly: true });
  try {
    const rows = db
      .prepare(
        `SELECT DISTINCT handle FROM (
           SELECT c.chat_identifier AS handle
           FROM conversations c
           JOIN messages m ON m.conversation_id = c.id
           WHERE c.account_id = ?
           UNION
           SELECT p.handle
           FROM participants p
           JOIN conversations c ON c.id = p.conversation_id
           JOIN messages m ON m.conversation_id = c.id
           WHERE c.account_id = ?
         )`,
      )
      .all(accountId, accountId) as Array<{ handle: string }>;

    const out = new Set<string>();
    for (const row of rows) {
      const handle = row.handle?.trim();
      if (!handle || handle.includes("@")) continue;
      const e164 = toPhoneE164(handle);
      if (e164) out.add(e164);
      else out.add(handle);
    }
    return out;
  } finally {
    db.close();
  }
}

function findContactIdByPhone(phone: string, accountId: string): number | null {
  const db = new Database(dbPath(), { readonly: true });
  try {
    const row = db
      .prepare(
        `SELECT contact_id FROM contact_handles WHERE account_id = ? AND handle = ?`,
      )
      .get(accountId, phone) as { contact_id: number } | undefined;
    return row?.contact_id ?? null;
  } finally {
    db.close();
  }
}

type MatchedCard = {
  index: number;
  preferredName: string;
  phones: string[];
  labels: string[];
};

function collectMatchedCards(
  text: string,
  messagePhones: Set<string>,
): {
  cardsTotal: number;
  skippedNoPhone: number;
  unmatched: number;
  matched: MatchedCard[];
} {
  const cards = parseVcfText(text);
  let skippedNoPhone = 0;
  let unmatched = 0;
  const matched: MatchedCard[] = [];

  for (let i = 0; i < cards.length; i++) {
    const card = cards[i]!;
    const draft = cardToDraft(card);
    const phones = normalizePhones(draft.phones);
    if (phones.length === 0) {
      skippedNoPhone += 1;
      continue;
    }
    if (!phones.some((p) => messagePhones.has(p))) {
      unmatched += 1;
      continue;
    }

    let preferredName = [
      draft.firstName.trim(),
      draft.middleName.trim(),
      draft.lastName.trim(),
    ]
      .filter(Boolean)
      .join(" ");
    if (!preferredName) {
      preferredName = card.fnRaw.trim() || phones[0]!;
    }

    matched.push({
      index: i,
      preferredName,
      phones,
      labels: draft.labels,
    });
  }

  return {
    cardsTotal: cards.length,
    skippedNoPhone,
    unmatched,
    matched,
  };
}

/**
 * Preview a VCF import: only cards whose phones appear in vault messages.
 * Does not write anything.
 */
export function previewContactsFromVcf(text: string): VcfImportPreview {
  const accountId = currentAccountId();
  const messagePhones = messagePhoneHandles(accountId);
  const collected = collectMatchedCards(text, messagePhones);

  const categoryCounts = new Map<string, { source: string; count: number }>();
  for (const card of collected.matched) {
    for (const label of card.labels) {
      const key = label.toLowerCase();
      const existing = categoryCounts.get(key);
      if (existing) {
        existing.count += 1;
      } else {
        categoryCounts.set(key, { source: label, count: 1 });
      }
    }
  }

  const categories = [...categoryCounts.values()]
    .sort((a, b) =>
      a.source.localeCompare(b.source, undefined, { sensitivity: "base" }),
    )
    .map((c) => ({ source: c.source, matchedCount: c.count }));

  return {
    cardsTotal: collected.cardsTotal,
    matched: collected.matched.length,
    unmatched: collected.unmatched,
    skippedNoPhone: collected.skippedNoPhone,
    categories,
  };
}

function resolveMappedLabels(
  sourceLabels: string[],
  mappings: VcfCategoryMapping[],
): string[] {
  const bySource = new Map<string, VcfCategoryMapping>();
  for (const m of mappings) {
    bySource.set(m.source.trim().toLowerCase(), m);
  }

  const out: string[] = [];
  const seen = new Set<string>();
  for (const source of sourceLabels) {
    const mapping = bySource.get(source.trim().toLowerCase());
    if (!mapping || !mapping.enabled) continue;
    const target = mapping.target.trim();
    if (!target) continue;
    if (isReservedLabelName(target)) {
      throw new Error(reservedLabelError(target));
    }
    const key = target.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(target);
  }
  return out;
}

function validateMappings(mappings: VcfCategoryMapping[]): void {
  const enabledTargets = new Map<string, string>();
  for (const m of mappings) {
    if (!m.enabled) continue;
    const target = m.target.trim();
    if (!target) {
      throw new Error(`Destination label required for category "${m.source}"`);
    }
    if (isReservedLabelName(target)) {
      throw new Error(reservedLabelError(target));
    }
    const key = target.toLowerCase();
    const prev = enabledTargets.get(key);
    if (prev && prev.toLowerCase() !== m.source.trim().toLowerCase()) {
      throw new Error(
        `Multiple categories map to the same label "${target}"`,
      );
    }
    enabledTargets.set(key, m.source);
  }
}

/**
 * Commit a VCF import for message-matched contacts only.
 * Re-parses the uploaded text server-side; applies only confirmed category mappings.
 * Merges without overwriting existing names. Additive / idempotent for labels.
 */
export function commitContactsFromVcf(
  text: string,
  mappings: VcfCategoryMapping[],
): VcfImportSummary {
  assertVaultWritable();
  validateMappings(mappings);

  const accountId = currentAccountId();
  const messagePhones = messagePhoneHandles(accountId);
  const collected = collectMatchedCards(text, messagePhones);

  const summary: VcfImportSummary = {
    cardsTotal: collected.cardsTotal,
    matched: collected.matched.length,
    unmatched: collected.unmatched,
    created: 0,
    updated: 0,
    skipped: collected.skippedNoPhone + collected.unmatched,
    errors: [],
  };

  for (const card of collected.matched) {
    try {
      const mappedLabels = resolveMappedLabels(card.labels, mappings);
      const owners = card.phones
        .map((p) => findContactIdByPhone(p, accountId))
        .filter((id): id is number => id != null);
      const uniqueOwners = [...new Set(owners)];

      if (uniqueOwners.length === 0) {
        createContact({
          preferredName: card.preferredName || null,
          phones: card.phones,
          labels: mappedLabels,
        });
        summary.created += 1;
        continue;
      }

      const intoId = uniqueOwners[0]!;
      if (uniqueOwners.length > 1) {
        summary.errors.push(
          `Card ${card.index + 1}: phones belong to multiple contacts; updated contact ${intoId} only`,
        );
      }

      const existing = getContact(intoId);
      if (!existing) {
        summary.errors.push(`Card ${card.index + 1}: contact ${intoId} missing`);
        summary.skipped += 1;
        continue;
      }

      const mergedPhones = [...existing.phones];
      for (const p of card.phones) {
        const owner = findContactIdByPhone(p, accountId);
        if (owner == null) {
          mergedPhones.push(p);
        }
      }

      // Never overwrite names the user (or prior import) already set.
      const nextPreferred =
        existing.preferredName?.trim() || card.preferredName || null;
      const nextLabels = [
        ...new Set([...existing.labels, ...mappedLabels]),
      ].sort((a, b) =>
        a.localeCompare(b, undefined, { sensitivity: "base" }),
      );

      const phonesChanged =
        mergedPhones.length !== existing.phones.length ||
        mergedPhones.some((p, idx) => p !== existing.phones[idx]);
      const namesChanged =
        nextPreferred !== (existing.preferredName ?? null);
      const labelsChanged =
        nextLabels.length !== existing.labels.length ||
        nextLabels.some((l) => !existing.labels.includes(l));

      if (phonesChanged || namesChanged || labelsChanged) {
        patchContact(intoId, {
          preferredName: nextPreferred,
          phones: phonesChanged ? mergedPhones : undefined,
          labels: labelsChanged ? nextLabels : undefined,
        });
        summary.updated += 1;
      } else {
        summary.skipped += 1;
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      summary.errors.push(`Card ${card.index + 1}: ${message}`);
      summary.skipped += 1;
    }
  }

  return summary;
}
