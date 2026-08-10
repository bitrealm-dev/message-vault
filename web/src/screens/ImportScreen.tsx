import { useState, useEffect } from "react";
import { useAuth } from "../lib/auth";
import { apiClient, getBaseUrl } from "../lib/api";
import { invokeExtract, invokeCancel, invokePush, awaitTauriJob } from "../lib/tauri";
import { isTauri } from "../lib/tauri-check";
import { EXPORT_SOURCES } from "../lib/exportSources";
import {
  getRememberImporterPaths,
  getImporterPath,
  setImporterPath,
  resolveImportStagingDir,
} from "../lib/system-settings";
import type { AttachmentMediaMode, ContactNameMode } from "../lib/types";
import PathPicker from "../components/PathPicker";
import PasswordField from "../components/PasswordField";
import StepProgress from "../components/StepProgress";
import Button from "../components/Button";
import Select, { ListBoxItem, selectItemClassName } from "../components/Select";
import {
  ATTACHMENT_OPTIONS,
  RESOLUTION_OPTIONS,
  fieldStyle,
  hintStyle,
  sectionGap,
  StackedField,
  CollapsibleSection,
  DateField,
} from "./import/ImportFormUi";

interface ImportStep {
  label: string;
  status: "pending" | "active" | "done" | "error";
  detail?: string;
}

const DEFAULT_SOURCE = "imessage-ios";

export default function ImportScreen() {
  const { token } = useAuth();
  const [source, setSource] = useState(DEFAULT_SOURCE);
  const [backupPath, setBackupPath] = useState(() =>
    getRememberImporterPaths() ? getImporterPath(DEFAULT_SOURCE) : "",
  );
  const [backupPassword, setBackupPassword] = useState("");
  const [showBackupPassword, setShowBackupPassword] = useState(false);
  const [attachmentMedia, setAttachmentMedia] = useState<AttachmentMediaMode>("copy");
  const [maxResolution, setMaxResolution] = useState("720p");
  const [maxFps, setMaxFps] = useState("30");
  const [minSizeMb, setMinSizeMb] = useState("20");
  const [contactNameMode, setContactNameMode] = useState<ContactNameMode>("fill_missing");
  const [formatOpen, setFormatOpen] = useState(true);
  const [filteringOpen, setFilteringOpen] = useState(false);
  const [processingOpen, setProcessingOpen] = useState(false);
  const [conversationFilter, setConversationFilter] = useState("");
  const [startDate, setStartDate] = useState("");
  const [endDate, setEndDate] = useState("");
  const [continueOnError, setContinueOnError] = useState(true);
  const [force, setForce] = useState(false);
  const [obfuscate, setObfuscate] = useState(false);

  const [running, setRunning] = useState(false);
  const [steps, setSteps] = useState<ImportStep[]>([
    { label: "Parse backup", status: "pending" },
    { label: "Convert attachments", status: "pending" },
    { label: "Upload to vault", status: "pending" },
  ]);
  const [showDetails, setShowDetails] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const [summary, setSummary] = useState("");
  const [phase, setPhase] = useState<"form" | "progress" | "done">("form");

  useEffect(() => {
    if (!getRememberImporterPaths()) return;
    setBackupPath(getImporterPath(source));
  }, [source]);

  const updateBackupPath = (path: string) => {
    setBackupPath(path);
    if (getRememberImporterPaths()) setImporterPath(source, path);
  };

  const isIos = source === "imessage-ios";
  const showCompress = isIos && attachmentMedia === "compress";
  const attachmentHelp: Record<AttachmentMediaMode, string> = {
    copy: "Copy all files as is",
    convert: "Convert all files to common formats (.jpg, .mp4, .mp3) at high quality",
    compress: "Re-encodes for smaller file size at the expense of some quality",
    skip: "Do not copy files",
  };

  const appendLog = (line: string) => {
    setLog((prev) => [...prev, line]);
  };

  const startImport = async () => {
    if (!isTauri()) return;
    setRunning(true);
    setPhase("progress");
    setLog([]);
    setShowDetails(true);
    setSteps([
      { label: "Parse backup", status: "active", detail: "Parsing backup…" },
      { label: "Convert attachments", status: "pending" },
      { label: "Upload to vault", status: "pending" },
    ]);

    try {
      const outputDir = await resolveImportStagingDir(backupPath, source);
      const baseUrl = getBaseUrl();
      if (!token) throw new Error("Not authenticated");

      // extract/push return when the background thread starts — wait for events.
      const extractSummary = await awaitTauriJob(
        () =>
          invokeExtract({
            source,
            path: backupPath,
            output_dir: outputDir,
            ...(isIos
              ? {
                  backup_password: backupPassword || undefined,
                  attachment_media: attachmentMedia,
                  media_max_resolution: maxResolution,
                  media_max_fps: maxFps,
                  media_min_size: `${minSizeMb.trim() || "20"}M`,
                  conversation_filter: conversationFilter || undefined,
                  start_date: startDate || undefined,
                  end_date: endDate || undefined,
                  obfuscate,
                }
              : {}),
          }),
        appendLog,
      );
      appendLog(extractSummary);

      setSteps((s) =>
        s.map((step, i) =>
          i === 0
            ? { ...step, status: "done", detail: "Extraction complete" }
            : i === 1
              ? { ...step, status: "done", detail: "Attachments processed" }
              : { ...step, status: "active", detail: "Uploading to vault…" },
        ),
      );

      const importSession = await apiClient.post<{ id: string }>("/v1/imports", {
        source,
        tool: "message-vault-io",
        mode: "append",
      });

      const pushSummary = await awaitTauriJob(
        () =>
          invokePush({
            base_url: baseUrl,
            username: "",
            key: token,
            input_dir: outputDir,
            mode: "append",
            force,
            continue_on_error: continueOnError,
            skip_attachments: false,
            trust_export: false,
            contact_name_mode: contactNameMode,
          }),
        appendLog,
      );
      appendLog(pushSummary);

      await apiClient.post(`/v1/imports/${importSession.id}/complete`, {});

      setSteps((s) =>
        s.map((step, i) =>
          i === 2 ? { ...step, status: "done", detail: "Upload complete" } : step,
        ),
      );
      setPhase("done");
      setSummary(pushSummary || "Import complete. Messages uploaded to vault.");
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setSteps((s) => s.map((step) => ({ ...step, status: "error" as const })));
      setLog((l) => [...l, `Error: ${msg}`]);
      setPhase("progress");
    } finally {
      setRunning(false);
    }
  };

  return (
    <div style={{ padding: "1.5rem", maxWidth: "640px" }}>
      {phase === "form" && (
        <>
          <h1 style={{ margin: "0 0 0.25rem 0", fontSize: "1.5rem", fontWeight: 700 }}>
            Import Messages
          </h1>
          <p style={{ margin: "0 0 1.25rem 0", color: "var(--muted)", fontSize: "0.875rem" }}>
            Select your messages.
          </p>

          <CollapsibleSection
            title="Import Messages"
            open={formatOpen}
            onToggle={() => setFormatOpen((o) => !o)}
          >
            <div style={sectionGap}>
              <Select
                selectedKey={source}
                onSelectionChange={(k) => setSource(String(k))}
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
                    value={backupPath}
                    onChange={updateBackupPath}
                    directory
                    placeholder="Path to the root of a device backup"
                  />
                </StackedField>

                <StackedField label="Encryption password (optional)">
                  <PasswordField
                    value={backupPassword}
                    onChange={setBackupPassword}
                    autoComplete="off"
                    showPassword={showBackupPassword}
                    onToggle={() => setShowBackupPassword((v) => !v)}
                  />
                </StackedField>

                <StackedField label="Attachments">
                  <Select
                    selectedKey={attachmentMedia}
                    onSelectionChange={(k) => setAttachmentMedia(k as AttachmentMediaMode)}
                    aria-label="Attachments"
                    triggerClassName="!bg-bg"
                  >
                    {ATTACHMENT_OPTIONS.map((o) => (
                      <ListBoxItem key={o.id} id={o.id} className={selectItemClassName}>
                        {o.label}
                      </ListBoxItem>
                    ))}
                  </Select>
                  <p style={hintStyle}>{attachmentHelp[attachmentMedia]}</p>
                </StackedField>

                {showCompress && (
                  <div style={{ marginLeft: "1rem", marginBottom: "1.1rem" }}>
                    <StackedField label="Target resolution">
                      <Select
                        selectedKey={maxResolution}
                        onSelectionChange={(k) => setMaxResolution(String(k))}
                        aria-label="Target resolution"
                        triggerClassName="!bg-bg"
                      >
                        {RESOLUTION_OPTIONS.map((r) => (
                          <ListBoxItem key={r} id={r} className={selectItemClassName}>
                            {r.replace("p", "")}
                          </ListBoxItem>
                        ))}
                      </Select>
                      <p style={hintStyle}>
                        Maximum video resolution; videos are not upscaled.
                      </p>
                    </StackedField>
                    <StackedField label="Max FPS">
                      <input
                        type="text"
                        value={maxFps}
                        onChange={(e) => setMaxFps(e.target.value)}
                        style={fieldStyle}
                      />
                      <p style={hintStyle}>
                        Maximum video frame rate; videos are not upscaled to this FPS.
                      </p>
                    </StackedField>
                    <StackedField label="Minimum Video File Size (Megabytes)">
                      <input
                        type="text"
                        value={minSizeMb}
                        onChange={(e) => setMinSizeMb(e.target.value)}
                        style={fieldStyle}
                      />
                      <p style={hintStyle}>Only re-encode videos above this size.</p>
                    </StackedField>
                  </div>
                )}

                <StackedField label="Contacts">
                  <Select
                    selectedKey={contactNameMode}
                    onSelectionChange={(k) => setContactNameMode(k as ContactNameMode)}
                    aria-label="Contacts"
                    triggerClassName="!bg-bg"
                  >
                    <ListBoxItem id="fill_missing" className={selectItemClassName}>
                      Fill in missing names using vault contacts
                    </ListBoxItem>
                    <ListBoxItem id="overwrite" className={selectItemClassName}>
                      Overwrite all import names with vault contacts
                    </ListBoxItem>
                  </Select>
                </StackedField>
              </>
            ) : (
              <StackedField label="Backup path">
                <PathPicker value={backupPath} onChange={updateBackupPath} directory />
              </StackedField>
            )}
          </CollapsibleSection>

          {isIos && (
            <CollapsibleSection
              title="Message Filtering"
              open={filteringOpen}
              onToggle={() => setFilteringOpen((o) => !o)}
            >
              <StackedField label="Participant Filtering">
                <input
                  type="text"
                  value={conversationFilter}
                  onChange={(e) => setConversationFilter(e.target.value)}
                  placeholder="Comma separate list of names and number"
                  style={fieldStyle}
                />
                <p style={hintStyle}>
                  Only conversations with the specified participants are imported, including
                  group conversations.
                </p>
              </StackedField>
              <div
                style={{
                  display: "flex",
                  gap: "0.75rem",
                  marginBottom: "1.1rem",
                  flexWrap: "wrap",
                }}
              >
                <DateField label="Start Date" value={startDate} onChange={setStartDate} />
                <DateField
                  label="End Date (exclusive)"
                  value={endDate}
                  onChange={setEndDate}
                />
              </div>
            </CollapsibleSection>
          )}

          <CollapsibleSection
            title="Processing Options (Advanced)"
            open={processingOpen}
            onToggle={() => setProcessingOpen((o) => !o)}
          >
            <label
              style={{
                display: "flex",
                alignItems: "center",
                gap: "0.5rem",
                fontSize: "0.875rem",
                marginBottom: "0.75rem",
              }}
            >
              <input
                type="checkbox"
                checked={continueOnError}
                onChange={(e) => setContinueOnError(e.target.checked)}
              />
              Continue importing after failed message conversion (default)
            </label>
            <label
              style={{
                display: "flex",
                alignItems: "center",
                gap: "0.5rem",
                fontSize: "0.875rem",
                marginBottom: "0.75rem",
              }}
            >
              <input
                type="checkbox"
                checked={force}
                onChange={(e) => setForce(e.target.checked)}
              />
              Force reprocessing
            </label>
            {isIos ? (
              <label
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "0.5rem",
                  fontSize: "0.875rem",
                  marginBottom: "0.5rem",
                }}
              >
                <input
                  type="checkbox"
                  checked={obfuscate}
                  onChange={(e) => setObfuscate(e.target.checked)}
                />
                Obfuscate - All message data is anonymized.
              </label>
            ) : null}
          </CollapsibleSection>

          <div style={{ display: "flex", gap: "0.75rem", marginTop: "0.5rem" }}>
            <Button
              variant="primary"
              onClick={startImport}
              disabled={!backupPath || running}
              style={{ padding: "0.55rem 1.5rem", borderRadius: "8px" }}
            >
              Import
            </Button>
          </div>
        </>
      )}

      {(phase === "progress" || phase === "done") && (
        <>
          <h1 style={{ margin: "0 0 1rem 0", fontSize: "1.5rem", fontWeight: 700 }}>
            Import Messages
          </h1>
          <StepProgress steps={steps} />
          <div style={{ marginTop: "1rem", display: "flex", gap: "0.75rem", alignItems: "center" }}>
            {running && (
              <Button onClick={() => invokeCancel()}>
                Cancel
              </Button>
            )}
            {!running && (
              <Button
                variant="ghost"
                onClick={() => {
                  setPhase("form");
                  setSummary("");
                  setLog([]);
                  setShowDetails(false);
                }}
                style={{ fontSize: "0.875rem", padding: "0.35rem 0.75rem" }}
              >
                ← Back
              </Button>
            )}
            <Button
              variant="ghost"
              onClick={() => setShowDetails(!showDetails)}
              style={{ fontSize: "0.813rem", padding: "0.25rem 0.5rem" }}
            >
              {showDetails ? "Hide details" : "Show details"}
            </Button>
          </div>
          {showDetails && (
            <pre
              style={{
                maxHeight: "300px",
                overflow: "auto",
                fontSize: "0.75rem",
                background: "var(--hover)",
                padding: "0.5rem",
                borderRadius: "4px",
                whiteSpace: "pre-wrap",
                wordBreak: "break-word",
              }}
            >
              {log.length === 0
                ? "No log entries"
                : log.map((line, i) => <div key={i}>{line}</div>)}
            </pre>
          )}
        </>
      )}

      {phase === "done" && (
        <>
          <div
            style={{
              marginTop: "1rem",
              padding: "1rem",
              background: "var(--ok-soft-bg)",
              borderRadius: "6px",
              fontSize: "0.875rem",
            }}
          >
            {summary}
          </div>
          <div style={{ marginTop: "1rem" }}>
            <Button
              variant="primary"
              onClick={() => {
                setPhase("form");
                setSummary("");
              }}
              style={{ padding: "0.5rem 1.5rem" }}
            >
              Import another
            </Button>
          </div>
        </>
      )}
    </div>
  );
}
