import Button from "../../components/Button";
import PasswordField from "../../components/PasswordField";
import PathPicker from "../../components/PathPicker";
import Select, { ListBoxItem, selectItemClassName } from "../../components/Select";
import { EXPORT_SOURCES } from "../../lib/exportSources";
import { parseSelectKey } from "../../lib/selectKey";
import type { AttachmentMediaMode, ContactNameMode } from "../../lib/types";
import {
  ATTACHMENT_OPTIONS,
  CollapsibleSection,
  DateField,
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
  formatOpen: boolean;
  onToggleFormat: () => void;
  filteringOpen: boolean;
  onToggleFiltering: () => void;
  processingOpen: boolean;
  onToggleProcessing: () => void;
  conversationFilter: string;
  onConversationFilterChange: (value: string) => void;
  startDate: string;
  onStartDateChange: (value: string) => void;
  endDate: string;
  onEndDateChange: (value: string) => void;
  force: boolean;
  onForceChange: (value: boolean) => void;
  obfuscate: boolean;
  onObfuscateChange: (value: boolean) => void;
  running: boolean;
  onImport: () => void;
};

const attachmentHelp: Record<AttachmentMediaMode, string> = {
  copy: "Copy all files as is",
  convert: "Convert all files to common formats (.jpg, .mp4, .mp3) at high quality",
  compress: "Re-encodes for smaller file size at the expense of some quality",
  skip: "Do not copy files",
};

export default function ImportFormFields(props: ImportFormFieldsProps) {
  const isIos = props.source === "imessage-ios";
  const showCompress = isIos && props.attachmentMedia === "compress";

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
            selectedKey={props.source}
            onSelectionChange={(k) => props.onSourceChange(String(k))}
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

        {isIos ? (
          <>
            <StackedField label="iPhone Backup Directory">
              <PathPicker
                value={props.backupPath}
                onChange={props.onBackupPathChange}
                directory
                placeholder="Path to the root of a device backup"
              />
            </StackedField>

            <StackedField label="Encryption password (optional)">
              <PasswordField
                aria-label="Encryption password"
                value={props.backupPassword}
                onChange={props.onBackupPasswordChange}
                autoComplete="new-password"
                showPassword={props.showBackupPassword}
                onToggle={props.onToggleBackupPassword}
              />
            </StackedField>

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

            {showCompress && (
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
          </>
        ) : (
          <StackedField label="Backup path">
            <PathPicker value={props.backupPath} onChange={props.onBackupPathChange} directory />
          </StackedField>
        )}
      </CollapsibleSection>

      {isIos && (
        <CollapsibleSection
          title="Message Filtering"
          open={props.filteringOpen}
          onToggle={props.onToggleFiltering}
        >
          <StackedField label="Participant Filtering">
            <input
              type="text"
              value={props.conversationFilter}
              onChange={(e) => props.onConversationFilterChange(e.target.value)}
              placeholder="Comma separate list of names and number"
              className={fieldStyle}
            />
            <p className={hintStyle}>
              Only conversations with the specified participants are imported, including group
              conversations.
            </p>
          </StackedField>
          <div className="mb-[1.1rem] flex flex-wrap gap-3">
            <DateField
              label="Start Date"
              value={props.startDate}
              onChange={props.onStartDateChange}
            />
            <DateField
              label="End Date (exclusive)"
              value={props.endDate}
              onChange={props.onEndDateChange}
            />
          </div>
        </CollapsibleSection>
      )}

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
        {isIos ? (
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
          onClick={props.onImport}
          disabled={!props.backupPath || props.running}
          className="!rounded-lg !px-6 !py-[0.55rem]"
        >
          Import
        </Button>
      </div>
    </>
  );
}
