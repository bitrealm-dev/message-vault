import { useState } from "react";
import { useAuth } from "../lib/auth";
import { apiClient, getBaseUrl } from "../lib/api";
import { invokeExtract, invokeCancel } from "../lib/tauri";
import { isTauri } from "../lib/tauri-check";
import { EXPORT_SOURCES } from "../lib/exportSources";
import type { AttachmentMediaMode, ContactNameMode } from "../lib/types";
import PathPicker from "../components/PathPicker";
import PasswordField from "../components/PasswordField";
import StepProgress from "../components/StepProgress";
import Button from "../components/Button";
import {
  ATTACHMENT_OPTIONS,
  RESOLUTION_OPTIONS,
  fieldStyle,
  hintStyle,
  sectionGap,
  StackedField,
  CollapsibleSection,
} from "./import/ImportFormUi";

interface ImportStep {
  label: string;
  status: "pending" | "active" | "done" | "error";
  detail?: string;
}

export default function ImportScreen() {
  const { token } = useAuth();
  const [source, setSource] = useState("imessage-ios");
  const [backupPath, setBackupPath] = useState("");
  const [backupPassword, setBackupPassword] = useState("");
  const [attachmentMedia, setAttachmentMedia] = useState<AttachmentMediaMode>("copy");
  const [maxResolution, setMaxResolution] = useState("720p");
  const [maxFps, setMaxFps] = useState("30");
  const [minSizeMb, setMinSizeMb] = useState("20");
  const [contactNameMode, setContactNameMode] = useState<ContactNameMode>("fill_missing");
  const [formatOpen, setFormatOpen] = useState(true);
  const [optionsOpen, setOptionsOpen] = useState(true);
  const [filteringOpen, setFilteringOpen] = useState(false);
  const [conversationFilter, setConversationFilter] = useState("");
  const [startDate, setStartDate] = useState("");
  const [endDate, setEndDate] = useState("");
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

  const isIos = source === "imessage-ios";
  const showCompress = isIos && attachmentMedia === "compress";

  const startImport = async () => {
    if (!isTauri()) return;
    setRunning(true);
    setPhase("progress");
    setLog([]);
    setSteps([
      { label: "Parse backup", status: "active", detail: "Parsing backup…" },
      { label: "Convert attachments", status: "pending" },
      { label: "Upload to vault", status: "pending" },
    ]);

    try {
      const outputDir = `${backupPath}/../extract-output`;
      await invokeExtract({
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
      });

      setSteps((s) =>
        s.map((step, i) =>
          i === 0
            ? { ...step, status: "done", detail: "Extraction complete" }
            : i === 1
              ? { ...step, status: "active", detail: "Processing attachments…" }
              : step
        )
      );
      setSteps((s) =>
        s.map((step, i) =>
          i === 1
            ? { ...step, status: "done", detail: "Attachments processed" }
            : i === 2
              ? { ...step, status: "active", detail: "Uploading to vault…" }
              : step
        )
      );

      const baseUrl = getBaseUrl();
      if (!token) throw new Error("Not authenticated");

      const importSession = await apiClient.post<{ id: string }>("/v1/imports", {
        source,
        tool: "message-vault-io",
        mode: "push",
      });

      const { invokePush } = await import("../lib/tauri");
      await invokePush({
        base_url: baseUrl,
        username: "",
        key: token,
        input_dir: outputDir,
        mode: "import",
        force: false,
        skip_attachments: false,
        trust_export: false,
        contact_name_mode: contactNameMode,
      });

      await apiClient.post(`/v1/imports/${importSession.id}/complete`, {});

      setSteps((s) =>
        s.map((step, i) =>
          i === 2 ? { ...step, status: "done", detail: "Upload complete" } : step
        )
      );
      setPhase("done");
      setSummary("Import complete. Messages uploaded to vault.");
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
            title="Import Format"
            open={formatOpen}
            onToggle={() => setFormatOpen((o) => !o)}
          >
            <div style={sectionGap}>
              <select
                value={source}
                onChange={(e) => setSource(e.target.value)}
                style={fieldStyle}
              >
                {EXPORT_SOURCES.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.label}
                  </option>
                ))}
              </select>
            </div>

            {isIos ? (
              <>
                <StackedField label="iPhone Backup Directory">
                  <PathPicker
                    value={backupPath}
                    onChange={setBackupPath}
                    directory
                    placeholder="Path to the root of a device backup"
                  />
                </StackedField>

                <StackedField label="Encryption password (optional)">
                  <PasswordField
                    value={backupPassword}
                    onChange={setBackupPassword}
                    autoComplete="off"
                  />
                </StackedField>
              </>
            ) : (
              <StackedField label="Backup path">
                <PathPicker value={backupPath} onChange={setBackupPath} directory />
              </StackedField>
            )}
          </CollapsibleSection>

          {isIos && (
            <>
              <CollapsibleSection
                title="Import Options"
                open={optionsOpen}
                onToggle={() => setOptionsOpen((o) => !o)}
              >
                <StackedField label="Message Attachments">
                  <select
                    value={attachmentMedia}
                    onChange={(e) => setAttachmentMedia(e.target.value as AttachmentMediaMode)}
                    style={fieldStyle}
                  >
                    {ATTACHMENT_OPTIONS.map((o) => (
                      <option key={o.id} value={o.id}>
                        {o.label}
                      </option>
                    ))}
                  </select>
                </StackedField>

                {showCompress && (
                  <div style={{ marginLeft: "1rem", marginBottom: "1.1rem" }}>
                    <StackedField label="Target resolution">
                      <select
                        value={maxResolution}
                        onChange={(e) => setMaxResolution(e.target.value)}
                        style={fieldStyle}
                      >
                        {RESOLUTION_OPTIONS.map((r) => (
                          <option key={r} value={r}>
                            {r.replace("p", "")}
                          </option>
                        ))}
                      </select>
                    </StackedField>
                    <StackedField label="Max FPS">
                      <input
                        type="text"
                        value={maxFps}
                        onChange={(e) => setMaxFps(e.target.value)}
                        style={fieldStyle}
                      />
                    </StackedField>
                    <StackedField label="Minimum file size (MB)">
                      <input
                        type="text"
                        value={minSizeMb}
                        onChange={(e) => setMinSizeMb(e.target.value)}
                        style={fieldStyle}
                      />
                    </StackedField>
                  </div>
                )}

                <StackedField label="Contacts">
                  <select
                    value={contactNameMode}
                    onChange={(e) => setContactNameMode(e.target.value as ContactNameMode)}
                    style={fieldStyle}
                  >
                    <option value="fill_missing">
                      Fill in missing names using vault contacts
                    </option>
                    <option value="overwrite">
                      Overwrite all import names with vault contacts
                    </option>
                  </select>
                </StackedField>
              </CollapsibleSection>

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
                    Only conversations with the specified participants are exported, including
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
                  <div style={{ flex: "1 1 12rem", minWidth: "10rem" }}>
                    <label style={{ display: "block", fontSize: "0.875rem", fontWeight: 500, marginBottom: "0.35rem" }}>
                      Start Date
                    </label>
                    <input
                      type="date"
                      value={startDate}
                      onChange={(e) => setStartDate(e.target.value)}
                      style={fieldStyle}
                    />
                  </div>
                  <div style={{ flex: "1 1 12rem", minWidth: "10rem" }}>
                    <label style={{ display: "block", fontSize: "0.875rem", fontWeight: 500, marginBottom: "0.35rem" }}>
                      End Date (exclusive)
                    </label>
                    <input
                      type="date"
                      value={endDate}
                      onChange={(e) => setEndDate(e.target.value)}
                      style={fieldStyle}
                    />
                  </div>
                </div>
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
              </CollapsibleSection>
            </>
          )}

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
          <div style={{ marginTop: "1rem", display: "flex", gap: "0.75rem" }}>
            {running && (
              <Button onClick={() => invokeCancel()}>
                Cancel
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
