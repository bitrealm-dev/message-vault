import Database from "better-sqlite3";
import { currentAccountId } from "./accountScope";
import { dbPath } from "./paths";
import { getContact, resetDb } from "./db";
import {
  appendContactsCsv,
  removeContactsCsv,
  rewriteCsvLabels,
  updateContactsCsv,
  updateContactsCsvLabelMembership,
} from "./contactsCsv";
import {
  isEmailHandle,
  phoneHandlesOnly,
  preferredPhoneHandle,
} from "./handleKind";
import { clearTrashedHandles } from "./handlesWrite";
import type { ContactDetail } from "./types";
import {
  isReservedLabelName,
  RESERVED_LABEL_NAMES,
  reservedLabelError,
} from "./reservedLabels";
import { assertNotOwnerHandle, assertVaultWritable } from "./owner";

export type ContactPatch = {
  exclude?: boolean;
  labels?: string[];
  firstName?: string | null;
  lastName?: string | null;
  phones?: string[];
};

function assertAllowedLabelName(name: string): void {
  if (isReservedLabelName(name)) {
    throw new Error(reservedLabelError(name));
  }
}

export type ContactCreate = {
  firstName?: string | null;
  lastName?: string | null;
  phones?: string[];
  exclude?: boolean;
  labels?: string[];
};

/** Insert a new contact in SQLite and append contacts.csv; returns the contact. */
export function createContact(input: ContactCreate): ContactDetail {
  assertVaultWritable();
  const accountId = currentAccountId();
  const firstName = input.firstName?.trim() || null;
  const lastName = input.lastName?.trim() || null;
  if (!firstName && !lastName) {
    throw new Error("first or last name required");
  }
  const phones = (input.phones ?? []).map((p) => p.trim()).filter(Boolean);
  const csvPhones = phoneHandlesOnly(phones);
  if (csvPhones.length === 0) {
    throw new Error(
      "at least one phone number required (emails alone cannot create a contact)",
    );
  }
  const exclude = input.exclude ?? false;
  const preferredHandle = preferredPhoneHandle(phones);
  const labels = (input.labels ?? [])
    .map((t) => t.trim())
    .filter(Boolean)
    .filter((t) => !RESERVED_LABEL_NAMES.has(t.toLowerCase()));

  for (const phone of phones) {
    assertNotOwnerHandle(phone);
  }

  let newId = 0;
  const writeDb = new Database(dbPath());
  try {
    const tx = writeDb.transaction(() => {
      for (const phone of phones) {
        const owner = phoneOwner(writeDb, phone, accountId);
        if (owner != null) {
          throw new Error(`phone ${phone} already belongs to another contact`);
        }
      }

      const result = writeDb
        .prepare(
          `INSERT INTO contacts (account_id, first_name, last_name, exclude, preferred_handle)
           VALUES (?, ?, ?, ?, ?)`,
        )
        .run(accountId, firstName, lastName, exclude ? 1 : 0, preferredHandle);
      newId = Number(result.lastInsertRowid);

      const insertPhone = writeDb.prepare(
        `INSERT INTO contact_handles (account_id, handle, contact_id) VALUES (?, ?, ?)`,
      );
      for (const phone of phones) {
        insertPhone.run(accountId, phone, newId);
      }
      clearTrashedHandles(writeDb, phones, accountId);

      if (labels.length > 0) {
        const insertMember = writeDb.prepare(
          `INSERT OR IGNORE INTO contact_label_members (contact_id, label_id) VALUES (?, ?)`,
        );
        for (const name of labels) {
          const groupId = ensureLabelId(writeDb, name, accountId);
          insertMember.run(newId, groupId);
        }
      }
    });
    tx();
  } finally {
    writeDb.close();
  }

  resetDb();
  appendContactsCsv({
    phones: csvPhones,
    firstName,
    lastName,
    exclude,
    groups: labels,
  });

  const created = getContact(newId);
  if (!created) {
    throw new Error("contact missing after create");
  }
  return created;
}

function ensureLabelId(
  db: Database.Database,
  name: string,
  accountId: string,
): number {
  assertAllowedLabelName(name);
  db.prepare(
    `INSERT OR IGNORE INTO contact_labels (account_id, name) VALUES (?, ?)`,
  ).run(accountId, name);
  const row = db
    .prepare(`SELECT id FROM contact_labels WHERE account_id = ? AND name = ?`)
    .get(accountId, name) as { id: number } | undefined;
  if (!row) throw new Error(`failed to ensure label ${name}`);
  return row.id;
}

function findLabelId(
  db: Database.Database,
  name: string,
  accountId: string,
): number | null {
  const row = db
    .prepare(`SELECT id FROM contact_labels WHERE account_id = ? AND name = ?`)
    .get(accountId, name) as { id: number } | undefined;
  return row?.id ?? null;
}

/** Add or remove one label for many contacts in one database transaction. */
export function setContactsLabelMembership(
  contactIds: number[],
  name: string,
  enable: boolean,
): number {
  assertVaultWritable();
  const accountId = currentAccountId();
  const ids = [
    ...new Set(contactIds.filter((id) => Number.isFinite(id) && id > 0)),
  ];
  if (ids.length === 0) throw new Error("contact ids required");
  const label = name.trim();
  if (!label) throw new Error("label name required");
  assertAllowedLabelName(label);

  const contacts = ids.map((id) => {
    const contact = getContact(id);
    if (!contact) throw new Error(`contact ${id} not found`);
    return contact;
  });
  const changedIds = new Set<number>();
  const writeDb = new Database(dbPath());
  try {
    const tx = writeDb.transaction(() => {
      const labelId = enable
        ? ensureLabelId(writeDb, label, accountId)
        : findLabelId(writeDb, label, accountId);
      if (labelId == null) return;
      const insert = writeDb.prepare(
        `INSERT OR IGNORE INTO contact_label_members (contact_id, label_id)
         SELECT id, ? FROM contacts WHERE id = ? AND account_id = ?`,
      );
      const remove = writeDb.prepare(
        `DELETE FROM contact_label_members
         WHERE contact_id = ? AND label_id = ?
           AND EXISTS (
             SELECT 1 FROM contacts
             WHERE contacts.id = contact_label_members.contact_id
               AND contacts.account_id = ?
           )`,
      );
      for (const id of ids) {
        const result = enable
          ? insert.run(labelId, id, accountId)
          : remove.run(id, labelId, accountId);
        if (result.changes > 0) changedIds.add(id);
      }
    });
    tx();
  } finally {
    writeDb.close();
  }

  if (changedIds.size === 0) return 0;
  resetDb();
  updateContactsCsvLabelMembership(
    contacts
      .filter((contact) => changedIds.has(contact.id))
      .map((contact) => ({
        phones: contact.phones,
        firstName: contact.firstName,
        lastName: contact.lastName,
      })),
    label,
    enable,
  );
  return changedIds.size;
}


export function createLabel(name: string): string {
  assertVaultWritable();
  const accountId = currentAccountId();
  const trimmed = name.trim();
  if (!trimmed) throw new Error("name required");
  assertAllowedLabelName(trimmed);

  const writeDb = new Database(dbPath());
  try {
    const existing = writeDb
      .prepare(
        `SELECT name FROM contact_labels WHERE account_id = ? AND name = ? COLLATE NOCASE`,
      )
      .get(accountId, trimmed) as { name: string } | undefined;
    if (existing) {
      throw new Error("label already exists");
    }
    writeDb
      .prepare(`INSERT INTO contact_labels (account_id, name) VALUES (?, ?)`)
      .run(accountId, trimmed);
  } finally {
    writeDb.close();
  }

  resetDb();
  return trimmed;
}

export function renameLabel(from: string, to: string): string {
  assertVaultWritable();
  const accountId = currentAccountId();
  const oldName = from.trim();
  const newName = to.trim();
  if (!oldName || !newName) throw new Error("name required");
  assertAllowedLabelName(newName);
  if (oldName.toLowerCase() === newName.toLowerCase()) {
    // Same name ignoring case — allow casing fix
    if (oldName === newName) return newName;
  }

  const writeDb = new Database(dbPath());
  try {
    const id = findLabelId(writeDb, oldName, accountId);
    if (id == null) throw new Error("label not found");

    const clash = writeDb
      .prepare(
        `SELECT id FROM contact_labels
         WHERE account_id = ? AND name = ? COLLATE NOCASE AND id != ?`,
      )
      .get(accountId, newName, id) as { id: number } | undefined;
    if (clash) throw new Error("label already exists");

    writeDb
      .prepare(`UPDATE contact_labels SET name = ? WHERE id = ? AND account_id = ?`)
      .run(newName, id, accountId);
  } finally {
    writeDb.close();
  }

  resetDb();
  rewriteCsvLabels((group) =>
    group.toLowerCase() === oldName.toLowerCase() ? newName : group,
  );
  return newName;
}

export function deleteLabel(name: string): void {
  assertVaultWritable();
  const accountId = currentAccountId();
  const trimmed = name.trim();
  if (!trimmed) throw new Error("name required");

  const writeDb = new Database(dbPath());
  try {
    const id = findLabelId(writeDb, trimmed, accountId);
    if (id == null) throw new Error("label not found");
    writeDb
      .prepare(`DELETE FROM contact_label_members WHERE label_id = ?`)
      .run(id);
    writeDb
      .prepare(`DELETE FROM contact_labels WHERE id = ? AND account_id = ?`)
      .run(id, accountId);
  } finally {
    writeDb.close();
  }

  resetDb();
  rewriteCsvLabels((group) =>
    group.toLowerCase() === trimmed.toLowerCase() ? null : group,
  );
}

function phoneOwner(
  db: Database.Database,
  phone: string,
  accountId: string,
): number | null {
  const row = db
    .prepare(
      `SELECT contact_id FROM contact_handles WHERE account_id = ? AND handle = ?`,
    )
    .get(accountId, phone) as { contact_id: number } | undefined;
  return row?.contact_id ?? null;
}

/**
 * Retarget message/conversation handles when a contact phone changes so the
 * person stays linked to their threads (list filters require phone↔message join).
 */
function remapPhoneHandle(
  db: Database.Database,
  contactId: number,
  from: string,
  to: string,
  accountId: string,
): void {
  if (from === to) return;

  const owner = phoneOwner(db, to, accountId);
  if (owner != null && owner !== contactId) {
    throw new Error(`phone ${to} already belongs to another contact`);
  }

  // Prefer updating the PK in place; if `to` already exists on this contact,
  // drop the old row instead (merge).
  if (owner === contactId) {
    db.prepare(
      `DELETE FROM contact_handles WHERE account_id = ? AND handle = ?`,
    ).run(accountId, from);
  } else {
    db.prepare(
      `UPDATE contact_handles SET handle = ? WHERE account_id = ? AND handle = ?`,
    ).run(to, accountId, from);
  }

  db.prepare(
    `UPDATE conversations SET chat_identifier = ?
     WHERE account_id = ? AND chat_identifier = ?`,
  ).run(to, accountId, from);
  db.prepare(`UPDATE participants SET handle = ? WHERE handle = ?`).run(to, from);
  db.prepare(`UPDATE messages SET sender = ? WHERE sender = ?`).run(to, from);
  db.prepare(`UPDATE tapbacks SET sender = ? WHERE sender = ?`).run(to, from);
}

function syncContactPhones(
  db: Database.Database,
  contactId: number,
  oldPhones: string[],
  newPhones: string[],
  accountId: string,
): void {
  const shared = Math.min(oldPhones.length, newPhones.length);
  for (let i = 0; i < shared; i++) {
    const from = oldPhones[i]!;
    const to = newPhones[i]!;
    if (from !== to) {
      remapPhoneHandle(db, contactId, from, to, accountId);
    }
  }

  for (let i = shared; i < oldPhones.length; i++) {
    db.prepare(
      `DELETE FROM contact_handles WHERE account_id = ? AND handle = ?`,
    ).run(accountId, oldPhones[i]);
  }

  const insert = db.prepare(
    `INSERT INTO contact_handles (account_id, handle, contact_id) VALUES (?, ?, ?)`,
  );
  for (let i = shared; i < newPhones.length; i++) {
    const phone = newPhones[i]!;
    const owner = phoneOwner(db, phone, accountId);
    if (owner != null && owner !== contactId) {
      throw new Error(`phone ${phone} already belongs to another contact`);
    }
    if (owner == null) {
      insert.run(accountId, phone, contactId);
    }
  }
}

/** Update contact fields in SQLite and contacts.csv; returns refreshed contact. */
export function patchContact(
  id: number,
  patch: ContactPatch,
): ContactDetail {
  assertVaultWritable();
  const accountId = currentAccountId();
  const existing = getContact(id);
  if (!existing) {
    throw new Error("contact not found");
  }

  const exclude = patch.exclude ?? existing.exclude;
  const labels = patch.labels ?? existing.labels;
  const firstName =
    patch.firstName !== undefined
      ? patch.firstName?.trim() || null
      : existing.firstName;
  const lastName =
    patch.lastName !== undefined
      ? patch.lastName?.trim() || null
      : existing.lastName;
  const phones =
    patch.phones !== undefined
      ? patch.phones.map((p) => p.trim()).filter(Boolean)
      : existing.phones;
  const preferredHandle = preferredPhoneHandle(phones);
  const csvPhones = phoneHandlesOnly(phones);
  const existingCsvPhones = phoneHandlesOnly(existing.phones);
  const csvPhonesChanged =
    patch.phones !== undefined &&
    (csvPhones.length !== existingCsvPhones.length ||
      csvPhones.some((p, i) => p !== existingCsvPhones[i]));

  if (patch.phones !== undefined && csvPhones.length === 0) {
    throw new Error(
      "at least one phone number required (emails alone cannot be a contact)",
    );
  }
  if (patch.phones !== undefined) {
    for (const phone of phones) {
      assertNotOwnerHandle(phone);
    }
  }

  const writeDb = new Database(dbPath());
  try {
    const tx = writeDb.transaction(() => {
      if (patch.phones) {
        syncContactPhones(writeDb, id, existing.phones, phones, accountId);
        clearTrashedHandles(writeDb, phones, accountId);
      }

      writeDb
        .prepare(
          `UPDATE contacts
           SET first_name = ?, last_name = ?, exclude = ?, preferred_handle = ?
           WHERE id = ? AND account_id = ?`,
        )
        .run(firstName, lastName, exclude ? 1 : 0, preferredHandle, id, accountId);

      if (patch.labels) {
        writeDb
          .prepare(`DELETE FROM contact_label_members WHERE contact_id = ?`)
          .run(id);
        const insert = writeDb.prepare(
          `INSERT OR IGNORE INTO contact_label_members (contact_id, label_id) VALUES (?, ?)`,
        );
        for (const name of labels) {
          const groupId = ensureLabelId(writeDb, name, accountId);
          insert.run(id, groupId);
        }
      }
    });
    tx();
  } finally {
    writeDb.close();
  }

  resetDb();
  updateContactsCsv(
    existingCsvPhones,
    { firstName: existing.firstName, lastName: existing.lastName },
    {
      exclude,
      groups: labels,
      firstName: patch.firstName !== undefined ? firstName : undefined,
      lastName: patch.lastName !== undefined ? lastName : undefined,
      phones: csvPhonesChanged ? csvPhones : undefined,
    },
  );

  const updated = getContact(id);
  if (!updated) {
    throw new Error("contact missing after update");
  }
  return updated;
}

/** Append a phone/email handle to an existing contact (for Unassigned assign). */
export function addPhoneToContact(id: number, phone: string): ContactDetail {
  assertVaultWritable();
  const accountId = currentAccountId();
  const existing = getContact(id);
  if (!existing) throw new Error("contact not found");
  const trimmed = phone.trim();
  if (!trimmed) throw new Error("phone required");
  assertNotOwnerHandle(trimmed);
  if (existing.phones.includes(trimmed)) return existing;

  // Emails live in SQLite only — never rewrite contacts.csv phones.
  if (isEmailHandle(trimmed)) {
    const writeDb = new Database(dbPath());
    try {
      const owner = phoneOwner(writeDb, trimmed, accountId);
      if (owner != null && owner !== id) {
        throw new Error(`phone ${trimmed} already belongs to another contact`);
      }
      if (owner == null) {
        writeDb
          .prepare(
            `INSERT INTO contact_handles (account_id, handle, contact_id) VALUES (?, ?, ?)`,
          )
          .run(accountId, trimmed, id);
        clearTrashedHandles(writeDb, [trimmed], accountId);
      }
    } finally {
      writeDb.close();
    }
    resetDb();
    const updated = getContact(id);
    if (!updated) throw new Error("contact missing after update");
    return updated;
  }

  return patchContact(id, { phones: [...existing.phones, trimmed] });
}

/**
 * Remove a phone/email handle from a contact. Does not delete conversations
 * or messages. Used to undo assign-from-unassigned.
 */
export function removePhoneFromContact(
  id: number,
  phone: string,
): ContactDetail {
  assertVaultWritable();
  const accountId = currentAccountId();
  const existing = getContact(id);
  if (!existing) throw new Error("contact not found");
  const trimmed = phone.trim();
  if (!trimmed) throw new Error("phone required");
  if (!existing.phones.includes(trimmed)) {
    throw new Error("handle not on contact");
  }

  if (isEmailHandle(trimmed)) {
    const writeDb = new Database(dbPath());
    try {
      const owner = phoneOwner(writeDb, trimmed, accountId);
      if (owner != null && owner !== id) {
        throw new Error(`phone ${trimmed} already belongs to another contact`);
      }
      writeDb
        .prepare(`DELETE FROM contact_handles WHERE account_id = ? AND handle = ?`)
        .run(accountId, trimmed);
      const preferred = preferredPhoneHandle(
        existing.phones.filter((p) => p !== trimmed),
      );
      writeDb
        .prepare(
          `UPDATE contacts SET preferred_handle = ? WHERE id = ? AND account_id = ?`,
        )
        .run(preferred, id, accountId);
    } finally {
      writeDb.close();
    }
    resetDb();
    const updated = getContact(id);
    if (!updated) throw new Error("contact missing after update");
    return updated;
  }

  const nextPhones = existing.phones.filter((p) => p !== trimmed);
  if (phoneHandlesOnly(nextPhones).length === 0) {
    throw new Error(
      "cannot remove last phone number (emails alone cannot be a contact)",
    );
  }
  return patchContact(id, { phones: nextPhones });
}

/**
 * Recreate a deleted group and re-attach member contacts.
 * Contacts that no longer exist are skipped.
 */
export function restoreLabel(
  name: string,
  memberContactIds: number[],
): string {
  assertVaultWritable();
  const trimmed = name.trim();
  if (!trimmed) throw new Error("name required");
  assertAllowedLabelName(trimmed);

  const created = createLabel(trimmed);
  for (const contactId of memberContactIds) {
    const contact = getContact(contactId);
    if (!contact) continue;
    if (contact.labels.some((g) => g.toLowerCase() === created.toLowerCase())) {
      continue;
    }
    patchContact(contactId, {
      labels: [...contact.labels, created].sort((a, b) =>
        a.localeCompare(b, undefined, { sensitivity: "base" }),
      ),
    });
  }
  return created;
}

/**
 * Create nameless contacts for 1:1 handles that still appear as Unassigned.
 * Returns how many contacts were created. No-op when the vault is read-only.
 */
export function ensureUnknownContacts(): number {
  try {
    assertVaultWritable();
  } catch {
    return 0;
  }
  const accountId = currentAccountId();
  // Dynamic import avoids circular deps with unassignedRead ↔ contactsRead.
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const { listUnassignedHandles } =
    require("./unassignedRead") as typeof import("./unassignedRead");
  const handles = listUnassignedHandles();
  if (handles.length === 0) return 0;

  const csvRows: string[][] = [];
  let created = 0;
  const writeDb = new Database(dbPath());
  try {
    const tx = writeDb.transaction(() => {
      for (const row of handles) {
        const handle = row.handle.trim();
        if (!handle) continue;
        const owner = phoneOwner(writeDb, handle, accountId);
        if (owner != null) continue;

        const result = writeDb
          .prepare(
            `INSERT INTO contacts (account_id, first_name, last_name, exclude, preferred_handle)
             VALUES (?, NULL, NULL, 0, ?)`,
          )
          .run(accountId, handle);
        const newId = Number(result.lastInsertRowid);
        writeDb
          .prepare(
            `INSERT INTO contact_handles (account_id, handle, contact_id) VALUES (?, ?, ?)`,
          )
          .run(accountId, handle, newId);
        clearTrashedHandles(writeDb, [handle], accountId);
        created += 1;

        if (!isEmailHandle(handle)) {
          const csvPhones = phoneHandlesOnly([handle]);
          if (csvPhones.length > 0) csvRows.push(csvPhones);
        }
      }
    });
    tx();
  } finally {
    writeDb.close();
  }
  resetDb();

  for (const phones of csvRows) {
    try {
      appendContactsCsv({
        phones,
        firstName: null,
        lastName: null,
        exclude: false,
        groups: [],
      });
    } catch (err) {
      console.error("ensureUnknownContacts CSV append failed", err);
    }
  }
  return created;
}

/**
 * Move all handles from a nameless source contact onto a named target, then
 * delete the source. Messages stay linked via handles.
 */
export function mergeContacts(fromId: number, intoId: number): ContactDetail {
  assertVaultWritable();
  if (fromId === intoId) throw new Error("cannot merge a contact into itself");

  const source = getContact(fromId);
  if (!source) throw new Error("source contact not found");
  const target = getContact(intoId);
  if (!target) throw new Error("target contact not found");

  const sourceHasName = Boolean(
    (source.firstName ?? "").trim() || (source.lastName ?? "").trim(),
  );
  if (sourceHasName) {
    throw new Error("only nameless contacts can be merged into another contact");
  }
  const targetHasName = Boolean(
    (target.firstName ?? "").trim() || (target.lastName ?? "").trim(),
  );
  if (!targetHasName) {
    throw new Error("merge target must have a name");
  }

  const accountId = currentAccountId();
  const sourceCsvPhones = phoneHandlesOnly(source.phones);
  const mergedPhones = [
    ...new Set([...target.phones, ...source.phones].map((p) => p.trim()).filter(Boolean)),
  ];
  const mergedCsvPhones = phoneHandlesOnly(mergedPhones);

  const writeDb = new Database(dbPath());
  try {
    const tx = writeDb.transaction(() => {
      for (const handle of source.phones) {
        const owner = phoneOwner(writeDb, handle, accountId);
        if (owner != null && owner !== fromId && owner !== intoId) {
          throw new Error(`handle ${handle} already belongs to another contact`);
        }
        if (owner === intoId) {
          writeDb
            .prepare(
              `DELETE FROM contact_handles WHERE account_id = ? AND handle = ? AND contact_id = ?`,
            )
            .run(accountId, handle, fromId);
          continue;
        }
        writeDb
          .prepare(
            `UPDATE contact_handles SET contact_id = ?
             WHERE account_id = ? AND handle = ? AND contact_id = ?`,
          )
          .run(intoId, accountId, handle, fromId);
      }
      writeDb
        .prepare(`DELETE FROM contact_label_members WHERE contact_id = ?`)
        .run(fromId);
      writeDb
        .prepare(`DELETE FROM contacts WHERE id = ? AND account_id = ?`)
        .run(fromId, accountId);

      const preferred =
        target.preferredHandle && mergedPhones.includes(target.preferredHandle)
          ? target.preferredHandle
          : preferredPhoneHandle(mergedPhones) ?? target.preferredHandle;
      writeDb
        .prepare(
          `UPDATE contacts SET preferred_handle = ? WHERE id = ? AND account_id = ?`,
        )
        .run(preferred, intoId, accountId);
    });
    tx();
  } finally {
    writeDb.close();
  }

  resetDb();
  if (sourceCsvPhones.length > 0) {
    removeContactsCsv([
      {
        phones: sourceCsvPhones,
        firstName: source.firstName,
        lastName: source.lastName,
      },
    ]);
  }
  if (mergedCsvPhones.length > 0) {
    updateContactsCsv(
      phoneHandlesOnly(target.phones),
      { firstName: target.firstName, lastName: target.lastName },
      {
        firstName: target.firstName,
        lastName: target.lastName,
        exclude: target.exclude,
        groups: target.labels,
        phones: mergedCsvPhones,
      },
    );
  }

  const updated = getContact(intoId);
  if (!updated) throw new Error("target missing after merge");
  return updated;
}

/** Delete contacts from SQLite and contacts.csv. */
export function deleteContacts(ids: number[]): number {
  assertVaultWritable();
  const accountId = currentAccountId();
  const unique = [...new Set(ids.filter((id) => Number.isFinite(id)))];
  if (unique.length === 0) return 0;

  const snapshots: Array<{
    phones: string[];
    firstName: string | null;
    lastName: string | null;
  }> = [];
  for (const id of unique) {
    const existing = getContact(id);
    if (!existing) continue;
    snapshots.push({
      phones: phoneHandlesOnly(existing.phones),
      firstName: existing.firstName,
      lastName: existing.lastName,
    });
  }
  if (snapshots.length === 0) {
    throw new Error("contact not found");
  }

  const writeDb = new Database(dbPath());
  try {
    const del = writeDb.prepare(
      `DELETE FROM contacts WHERE id = ? AND account_id = ?`,
    );
    const tx = writeDb.transaction(() => {
      for (const id of unique) {
        del.run(id, accountId);
      }
    });
    tx();
  } finally {
    writeDb.close();
  }

  resetDb();
  removeContactsCsv(snapshots);
  return snapshots.length;
}
