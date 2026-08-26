import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import Button from "../../components/Button";
import PasswordField from "../../components/PasswordField";
import PathPicker from "../../components/PathPicker";
import PhoneTokenField, { type PhoneTokenFieldHandle } from "../../components/PhoneTokenField";
import Select, { ListBoxItem, selectItemClassName } from "../../components/Select";
import { EXPORT_SOURCES } from "../../lib/exportSources";
import {
  IMESSAGE_SOURCE_ID,
  type ImessagePathStats,
  imessageAttachmentRootRequired,
  imessageCanImport,
  imessageShowsAppleContacts,
  imessageShowsAttachmentRoot,
  imessageShowsPassword,
  imessageVisiblePlatforms,
  isImessageMethod,
} from "../../lib/imessageImport";
import { ownerPhonesNeedMismatchAck } from "../../lib/phoneTokens";
import { parseSelectKey } from "../../lib/selectKey";
import type { AttachmentMediaMode, ContactNameMode } from "../../lib/types";
import { accentLink } from "../../lib/uiStyles";
import {
  ATTACHMENT_OPTIONS,
  CollapsibleSection,
  fieldStyle,
  hintStyle,
  RESOLUTION_OPTIONS,
  StackedField,
  sectionGap,
} from "./ImportFormUi";

export type ImportFormFieldsProps = {
  source: string;
  onSourceChange: (source: string) => void;
  backupPath: string;
  onBackupPathChange: (path: string) => void;
  backupPassword: string;
  onBackupPasswordChange: (value: string) => void;
  showBackupPassword: boolean;
  onToggleBackupPassword: () => void;
  attachmentRoot: string;
  onAttachmentRootChange: (path: string) => void;
  appleContacts: string;
  onAppleContactsChange: (path: string) => void;
  pathStats: ImessagePathStats;
  attachmentMedia: AttachmentMediaMode;
  onAttachmentMediaChange: (mode: AttachmentMediaMode) => void;
  maxResolution: string;
  onMaxResolutionChange: (value: string) => void;
  maxFps: string;
  onMaxFpsChange: (value: string) => void;
  minSizeMb: string;
  onMinSizeMbChange: (value: string) => void;
  contactNameMode: ContactNameMode;
  onContactNameModeChange: (mode: ContactNameMode) => void;
  ownerPhones: string[];
  onOwnerPhonesChange: (phones: string[]) => void;
  /** Vault account phones for SBR mismatch checks (empty until loaded). */
  profilePhones: string[];
  profilePhonesReady: boolean;
  /** True when the profile request failed (fail open on mismatch gate). */
  profilePhonesError: boolean;
  showMissingAccountPhoneWarning: boolean;
  formatOpen: boolean;
  onToggleFormat: () => void;
  processingOpen: boolean;
  onToggleProcessing: () => void;
  force: boolean;
  onForceChange: (value: boolean) => void;
  obfuscate: boolean;
  onObfuscateChange: (value: boolean) => void;
  running: boolean;
  /** Optional flushed owner phones (SBR commits draft before import). */
  onImport: (ownerPhones?: string[]) => void;
};

const SQLITE_DB_FILTERS = [{ name: "SQLite database", extensions: ["db"] }];
const APPLE_CONTACTS_FILTERS = [{ name: "Apple AddressBook", extensions: ["abcddb", "sqlitedb"] }];

const ATTACHMENT_FOLDER_HINT_MAC =
  "Leave empty if Attachments and StickerCache are next to chat.db. Set this only when those folders live somewhere else.";
const ATTACHMENT_FOLDER_HINT_JAILBREAK = "Folder that contains Attachments and StickerCache.";
const APPLE_CONTACTS_HINT_MAC =
  "Default: use the local AddressBook. Pick AddressBook-v22.abcddb or AddressBook.sqlitedb only if that file is not in the usual Contacts location.";
const APPLE_CONTACTS_HINT_JAILBREAK =
  "AddressBook-v22.abcddb or AddressBook.sqlitedb. A local Mac AddressBook scan will not find a phone copy.";

const attachmentHelp: Record<AttachmentMediaMode, string> = {
  copy: "Copy all files as is",
  convert: "Convert all files to common formats (.jpg, .mp4, .mp3) at high quality",
  compress: "Re-encodes for smaller file size at the expense of some quality",
  skip: "Do not copy files",
};

function FieldStatus({ message }: { message: string | undefined }) {
  if (!message) return null;
  return (
    <p className={hintStyle} role="status">
      {message}
    </p>
  );
}

function AttachmentFields(props: {
  attachmentMedia: AttachmentMediaMode;
  onAttachmentMediaChange: (mode: AttachmentMediaMode) => void;
  showCompress: boolean;
  maxResolution: string;
  onMaxResolutionChange: (value: string) => void;
  maxFps: string;
  onMaxFpsChange: (value: string) => void;
  minSizeMb: string;
  onMinSizeMbChange: (value: string) => void;
}) {
  return (
    <>
      <StackedField label="Attachments">
        <Select
          selectedKey={props.attachmentMedia}
          onSelectionChange={(k) => {
            const mode = parseSelectKey(k, ["copy", "convert", "compress", "skip"] as const);
            if (mode) props.onAttachmentMediaChange(mode);
          }}
          aria-label="Attachments"
          triggerClassName="!bg-bg"
        >
          {ATTACHMENT_OPTIONS.map((o) => (
            <ListBoxItem key={o.id} id={o.id} className={selectItemClassName}>
              {o.label}
            </ListBoxItem>
          ))}
        </Select>
        <p className={hintStyle}>{attachmentHelp[props.attachmentMedia]}</p>
      </StackedField>

      {props.showCompress && (
        <div className="mb-[1.1rem] ml-4">
          <StackedField label="Target resolution">
            <Select
              selectedKey={props.maxResolution}
              onSelectionChange={(k) => props.onMaxResolutionChange(String(k))}
              aria-label="Target resolution"
              triggerClassName="!bg-bg"
            >
              {RESOLUTION_OPTIONS.map((r) => (
                <ListBoxItem key={r} id={r} className={selectItemClassName}>
                  {r.replace("p", "")}
                </ListBoxItem>
              ))}
            </Select>
            <p className={hintStyle}>Maximum video resolution; videos are not upscaled.</p>
          </StackedField>
          <StackedField label="Max FPS">
            <input
              type="text"
              value={props.maxFps}
              onChange={(e) => props.onMaxFpsChange(e.target.value)}
              className={fieldStyle}
            />
            <p className={hintStyle}>
              Maximum video frame rate; videos are not upscaled to this FPS.
            </p>
          </StackedField>
          <StackedField label="Minimum Video File Size (Megabytes)">
            <input
              type="text"
              value={props.minSizeMb}
              onChange={(e) => props.onMinSizeMbChange(e.target.value)}
              className={fieldStyle}
            />
            <p className={hintStyle}>Only re-encode videos above this size.</p>
          </StackedField>
        </div>
      )}
    </>
  );
}

function ContactsField(props: {
  contactNameMode: ContactNameMode;
  onContactNameModeChange: (mode: ContactNameMode) => void;
}) {
  return (
    <StackedField label="Contacts">
      <Select
        selectedKey={props.contactNameMode}
        onSelectionChange={(k) => {
          const mode = parseSelectKey(k, ["fill_missing", "overwrite", "as_is"] as const);
          if (mode) props.onContactNameModeChange(mode);
        }}
        aria-label="Contacts"
        triggerClassName="!bg-bg"
      >
        <ListBoxItem id="fill_missing" className={selectItemClassName}>
          Fill in missing names using vault contacts
        </ListBoxItem>
        <ListBoxItem id="overwrite" className={selectItemClassName}>
          Overwrite all import names with vault contacts
        </ListBoxItem>
        <ListBoxItem id="as_is" className={selectItemClassName}>
          Leave unknown names as is
        </ListBoxItem>
      </Select>
    </StackedField>
  );
}

export default function ImportFormFields(props: ImportFormFieldsProps) {
  const isIos = props.source === "imessage-ios";
  const isSbr = props.source === "sms-backup-restore";
  const imessageMethod = isImessageMethod(props.source) ? props.source : null;
  const imessageGate = imessageMethod
    ? imessageCanImport({
        method: imessageMethod,
        backupPath: props.backupPath,
        attachmentRoot: props.attachmentRoot,
        appleContacts: props.appleContacts,
        backupPassword: props.backupPassword,
        stats: props.pathStats,
      })
    : null;
  const imessageErrors = imessageGate?.errors ?? {};
  const showCompress = (imessageMethod !== null || isSbr) && props.attachmentMedia === "compress";
  const phoneFieldRef = useRef<PhoneTokenFieldHandle>(null);
  const [phoneDraft, setPhoneDraft] = useState("");
  const [mismatchAck, setMismatchAck] = useState(false);
  const phoneDraftPending = phoneDraft.trim().length > 0;
  const phonesForMatch = phoneDraftPending
    ? [...props.ownerPhones, phoneDraft.trim()]
    : props.ownerPhones;
  const phonesMismatch =
    isSbr &&
    ownerPhonesNeedMismatchAck(phonesForMatch, props.profilePhones, {
      ready: props.profilePhonesReady,
      fetchFailed: props.profilePhonesError,
    });

  useEffect(() => {
    if (!phonesMismatch) setMismatchAck(false);
  }, [phonesMismatch]);

  useEffect(() => {
    if (!isSbr) {
      setPhoneDraft("");
      setMismatchAck(false);
    }
  }, [isSbr]);

  const canImport = imessageGate
    ? imessageGate.enabled && !props.running
    : Boolean(props.backupPath) &&
      !props.running &&
      (!isSbr || props.profilePhonesReady) &&
      (!isSbr || props.ownerPhones.length > 0 || phoneDraftPending) &&
      (!phonesMismatch || mismatchAck);

  function handleImport(): void {
    if (isSbr) {
      if (!props.profilePhonesReady) return;
      const phones = phoneFieldRef.current?.flush() ?? props.ownerPhones;
      if (phones.length === 0) return;
      const mismatch = ownerPhonesNeedMismatchAck(phones, props.profilePhones, {
        ready: props.profilePhonesReady,
        fetchFailed: props.profilePhonesError,
      });
      if (mismatch && !mismatchAck) return;
      props.onImport(phones);
      return;
    }
    props.onImport();
  }

  return (
    <>
      <h1 className="m-0 mb-1 text-2xl font-bold">Import Messages</h1>
      <p className="m-0 mb-5 text-[0.875rem] text-muted">Select your messages.</p>

      <CollapsibleSection
        title="Import Messages"
        open={props.formatOpen}
        onToggle={props.onToggleFormat}
      >
        <div className={sectionGap}>
          <Select
            selectedKey={imessageMethod ? IMESSAGE_SOURCE_ID : props.source}
            onSelectionChange={(k) => {
              props.onSourceChange(String(k));
            }}
            aria-label="Import source"
            triggerClassName="!bg-bg"
          >
            {EXPORT_SOURCES.map((s) => (
              <ListBoxItem key={s.id} id={s.id} className={selectItemClassName}>
                {s.label}
              </ListBoxItem>
            ))}
          </Select>
        </div>

        {imessageMethod ? (
          <StackedField label="Platform">
            <Select
              selectedKey={props.source}
              onSelectionChange={(k) => {
                const key = String(k);
                if (isImessageMethod(key)) props.onSourceChange(key);
              }}
              aria-label="Platform"
              triggerClassName="!bg-bg"
            >
              {imessageVisiblePlatforms(imessageMethod).map((m) => (
                <ListBoxItem key={m.id} id={m.id} className={selectItemClassName}>
                  {m.label}
                </ListBoxItem>
              ))}
            </Select>
          </StackedField>
        ) : null}

        {imessageMethod ? (
          <>
            {imessageMethod === "imessage-ios" ? (
              <StackedField label="iPhone Backup Directory" required>
                <PathPicker
                  value={props.backupPath}
                  onChange={props.onBackupPathChange}
                  directory
                  placeholder="Path to the root of a device backup"
                />
                <FieldStatus message={imessageErrors.backupPath} />
              </StackedField>
            ) : (
              <StackedField label="Messages database" required>
                <PathPicker
                  value={props.backupPath}
                  onChange={props.onBackupPathChange}
                  placeholder={
                    imessageMethod === "imessage-macos" ? "Path to chat.db" : "Path to sms.db"
                  }
                  filters={SQLITE_DB_FILTERS}
                />
                <FieldStatus message={imessageErrors.backupPath} />
              </StackedField>
            )}

            {imessageShowsPassword(imessageMethod) ? (
              <StackedField
                label="Encryption password"
                required={props.pathStats.backupEncrypted === true}
                optional={props.pathStats.backupEncrypted !== true}
              >
                <PasswordField
                  aria-label={
                    props.pathStats.backupEncrypted === true
                      ? "Encryption password"
                      : "Encryption password (Optional)"
                  }
                  value={props.backupPassword}
                  onChange={props.onBackupPasswordChange}
                  autoComplete="new-password"
                  showPassword={props.showBackupPassword}
                  onToggle={props.onToggleBackupPassword}
                />
                <FieldStatus message={imessageErrors.backupPassword} />
              </StackedField>
            ) : null}

            {imessageShowsAttachmentRoot(imessageMethod) ? (
              <StackedField
                label="Attachment folder"
                required={imessageAttachmentRootRequired(imessageMethod)}
                optional={!imessageAttachmentRootRequired(imessageMethod)}
              >
                <PathPicker
                  value={props.attachmentRoot}
                  onChange={props.onAttachmentRootChange}
                  directory
                />
                <p className={hintStyle}>
                  {imessageMethod === "imessage-macos"
                    ? ATTACHMENT_FOLDER_HINT_MAC
                    : ATTACHMENT_FOLDER_HINT_JAILBREAK}
                </p>
                <FieldStatus message={imessageErrors.attachmentRoot} />
              </StackedField>
            ) : null}

            {imessageShowsAppleContacts(imessageMethod) ? (
              <StackedField label="Apple Contacts file" optional>
                <PathPicker
                  value={props.appleContacts}
                  onChange={props.onAppleContactsChange}
                  filters={APPLE_CONTACTS_FILTERS}
                />
                <p className={hintStyle}>
                  {imessageMethod === "imessage-macos"
                    ? APPLE_CONTACTS_HINT_MAC
                    : APPLE_CONTACTS_HINT_JAILBREAK}
                </p>
                <FieldStatus message={imessageErrors.appleContacts} />
              </StackedField>
            ) : null}

            <AttachmentFields
              attachmentMedia={props.attachmentMedia}
              onAttachmentMediaChange={props.onAttachmentMediaChange}
              showCompress={showCompress}
              maxResolution={props.maxResolution}
              onMaxResolutionChange={props.onMaxResolutionChange}
              maxFps={props.maxFps}
              onMaxFpsChange={props.onMaxFpsChange}
              minSizeMb={props.minSizeMb}
              onMinSizeMbChange={props.onMinSizeMbChange}
            />

            <ContactsField
              contactNameMode={props.contactNameMode}
              onContactNameModeChange={props.onContactNameModeChange}
            />
          </>
        ) : isSbr ? (
          <>
            <StackedField label="Backup Directory">
              <PathPicker
                value={props.backupPath}
                onChange={props.onBackupPathChange}
                directory
                placeholder="Folder containing sms-*.xml backup files"
              />
              <p className={hintStyle}>
                Point at a folder of SMS Backup &amp; Restore XML files (not a single ZIP). Unlock
                encrypted backups before selecting the folder.
              </p>
            </StackedField>

            <AttachmentFields
              attachmentMedia={props.attachmentMedia}
              onAttachmentMediaChange={props.onAttachmentMediaChange}
              showCompress={showCompress}
              maxResolution={props.maxResolution}
              onMaxResolutionChange={props.onMaxResolutionChange}
              maxFps={props.maxFps}
              onMaxFpsChange={props.onMaxFpsChange}
              minSizeMb={props.minSizeMb}
              onMinSizeMbChange={props.onMinSizeMbChange}
            />

            <ContactsField
              contactNameMode={props.contactNameMode}
              onContactNameModeChange={props.onContactNameModeChange}
            />

            <StackedField label="Backup Device Phone Numbers">
              <PhoneTokenField
                ref={phoneFieldRef}
                value={props.ownerPhones}
                onChange={props.onOwnerPhonesChange}
                onDraftChange={setPhoneDraft}
                aria-label="Backup Device Phone Numbers"
              />
              <p className={hintStyle}>
                Pre-filled from your profile. Add numbers from other SIMs, if needed.
              </p>
              {props.showMissingAccountPhoneWarning ? (
                <div
                  role="status"
                  className="mt-2 rounded-lg border border-warn-soft-border bg-warn-soft-bg px-3 py-2 text-[0.8125rem] text-warn-soft-text"
                >
                  Your user profile is missing a phone number. Add one in{" "}
                  <Link to="/settings?tab=profile" className={`${accentLink} text-[0.8125rem]`}>
                    Settings → Profile
                  </Link>{" "}
                  so import can tell which messages you sent.
                </div>
              ) : null}
              {phonesMismatch && !mismatchAck && phonesForMatch.length > 0 ? (
                <div
                  role="status"
                  className="mt-2 rounded-lg border border-warn-soft-border bg-warn-soft-bg px-3 py-2 text-[0.8125rem] text-warn-soft-text"
                >
                  I understand none of the entered phone numbers match my profile and that imported
                  messages will not be linked to my account.
                </div>
              ) : null}
              <label className="mt-2 flex cursor-pointer items-start gap-2 text-[0.8125rem] text-text">
                <input
                  type="checkbox"
                  className="mt-0.5 shrink-0"
                  checked={mismatchAck}
                  onChange={(e) => setMismatchAck(e.target.checked)}
                />
                <span>Allow import from phone numbers not on my profile.</span>
              </label>
            </StackedField>
          </>
        ) : (
          <StackedField label="Backup path">
            <PathPicker value={props.backupPath} onChange={props.onBackupPathChange} directory />
          </StackedField>
        )}
      </CollapsibleSection>

      <CollapsibleSection
        title="Processing Options (Advanced)"
        open={props.processingOpen}
        onToggle={props.onToggleProcessing}
      >
        <label className="mb-3 flex items-center gap-2 text-[0.875rem]">
          <input
            type="checkbox"
            checked={props.force}
            onChange={(e) => props.onForceChange(e.target.checked)}
          />
          Force reprocessing
        </label>
        {isIos || isSbr ? (
          <label className="mb-2 flex items-center gap-2 text-[0.875rem]">
            <input
              type="checkbox"
              checked={props.obfuscate}
              onChange={(e) => props.onObfuscateChange(e.target.checked)}
            />
            Obfuscate - All message data is anonymized.
          </label>
        ) : null}
      </CollapsibleSection>

      <div className="mt-2 flex gap-3">
        <Button
          variant="primary"
          onClick={handleImport}
          disabled={!canImport}
          className="!rounded-lg !px-6 !py-[0.55rem]"
        >
          Import
        </Button>
      </div>
    </>
  );
}
