import { useState, useEffect, useRef } from "react";
import { useAuth } from "../lib/auth";
import { apiClient, getBaseUrl } from "../lib/api";
import {
  invokeExtract,
  invokeCancel,
  invokePush,
  awaitTauriJob,
  type TauriJobResult,
} from "../lib/tauri";
import { isTauri } from "../lib/tauri-check";
import { EXPORT_SOURCES } from "../lib/exportSources";
import {
  getRememberImporterPaths,
  getImporterPath,
  setImporterPath,
  resolveImportStagingDir,
} from "../lib/system-settings";
import type {
  AttachmentMediaMode,
  ContactNameMode,
  ImportIssueEvent,
  ImportProgressEvent,
} from "../lib/types";
import PathPicker from "../components/PathPicker";
import PasswordField from "../components/PasswordField";
import StepProgress from "../components/StepProgress";
import Button from "../components/Button";
import ImportSummaryPanel, {
  type ImportIssue,
  type ImportSummaryView,
} from "../components/import/ImportSummaryPanel";
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

function formatDuration(milliseconds: number): string {
  const seconds = Math.max(0, Math.round(milliseconds / 1000));
  const minutes = Math.floor(seconds / 60);
  return minutes > 0 ? `${minutes}m ${seconds % 60}s` : `${seconds}s`;
}

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
  const [phase, setPhase] = useState<"form" | "progress" | "done">("form");
  const [elapsedMs, setElapsedMs] = useState(0);
  const [summaryView, setSummaryView] = useState<ImportSummaryView | null>(null);
  const startedAtRef = useRef<number | null>(null);
  const activeStepRef = useRef<ImportIssue["step"]>("parse");
  const issuesRef = useRef<ImportIssue[]>([]);
  const countsRef = useRef<{
    parseMessages?: number;
    convertDetail?: string;
    uploadFiles?: number;
  }>({});
  const timingRef = useRef<{
    extractStartedAt: number | null;
    parseStartedAt: number | null;
    parseEndedAt: number | null;
    convertStartedAt: number | null;
    convertEndedAt: number | null;
  }>({
    extractStartedAt: null,
    parseStartedAt: null,
    parseEndedAt: null,
    convertStartedAt: null,
    convertEndedAt: null,
  });

  useEffect(() => {
    if (!getRememberImporterPaths()) return;
    setBackupPath(getImporterPath(source));
  }, [source]);

  useEffect(() => {
    if (!running || startedAtRef.current == null) return;

    const timer = window.setInterval(() => {
      setElapsedMs(performance.now() - (startedAtRef.current ?? performance.now()));
    }, 250);
    return () => window.clearInterval(timer);
  }, [running]);

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

  const applyProgress = (event: ImportProgressEvent) => {
    const now = performance.now();
    if (event.step === "parse") {
      activeStepRef.current = "parse";
      timingRef.current.parseStartedAt ??= now;
      countsRef.current.parseMessages =
        event.total > 0 && event.done >= event.total ? event.total : event.done;
    } else if (event.step === "convert") {
      activeStepRef.current = "convert";
      countsRef.current.convertDetail = "Included in extract";
    } else {
      activeStepRef.current = "upload";
      countsRef.current.uploadFiles =
        event.total > 0 && event.done >= event.total - 1 ? event.total : event.done;
    }

    const stepIndex = event.step === "parse" ? 0 : event.step === "convert" ? 1 : 2;
    const rawDetail =
      event.status === "included_in_extract"
        ? "Included in extract"
        : event.status
          ? `${event.done}/${event.total} (${event.status})`
          : `${event.done}/${event.total}`;

    setSteps((current) =>
      current.map((step, index) => {
        if (index < stepIndex) {
          return { ...step, status: "done", detail: step.detail ?? undefined };
        }
        if (index > stepIndex) {
          return step;
        }
        const done = event.total > 0 && event.done >= event.total;
        const verb =
          event.step === "upload" ? "Uploading" : event.step === "convert" ? "Converting" : "Parsing";
        return {
          ...step,
          status: done ? "done" : "active",
          detail:
            event.status === "included_in_extract" && event.step === "convert"
              ? rawDetail
              : `${verb} ${rawDetail}`,
        };
      }),
    );
  };

  const recordIssue = (issue: ImportIssueEvent) => {
    issuesRef.current = [...issuesRef.current, issue];
    appendLog(`${issue.kind === "skip" ? "Skipped" : "Error"}: ${issue.item}: ${issue.reason}`);
  };

  const startImport = async () => {
    if (!isTauri()) return;
    const importStartedAt = performance.now();
    startedAtRef.current = importStartedAt;
    activeStepRef.current = "parse";
    issuesRef.current = [];
    countsRef.current = {};
    timingRef.current = {
      extractStartedAt: null,
      parseStartedAt: null,
      parseEndedAt: null,
      convertStartedAt: null,
      convertEndedAt: null,
    };
    setRunning(true);
    setPhase("progress");
    setElapsedMs(0);
    setLog([]);
    setSummaryView(null);
    setShowDetails(true);
    setSteps([
      { label: "Parse backup", status: "active", detail: "Parsing backup…" },
      { label: "Convert attachments", status: "pending" },
      { label: "Upload to vault", status: "pending" },
    ]);

    let importSessionId: number | null = null;
    let importCompleted = false;
    let parseMs: number | null = null;
    let convertMs: number | null = null;
    let uploadMs: number | null = null;
    let pushResult: TauriJobResult | null = null;
    try {
      const baseUrl = getBaseUrl();
      if (!token) throw new Error("Not authenticated");

      const importSession = await apiClient.post<{ id: number }>("/v1/imports", {
        source,
        tool: "message-vault-io",
        mode: "append",
      });
      importSessionId = importSession.id;

      const outputDir = await resolveImportStagingDir(backupPath, source);
      // extract/push return when the background thread starts — wait for events.
      timingRef.current.extractStartedAt = performance.now();
      const extractResult = await awaitTauriJob(
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
        applyProgress,
        recordIssue,
      );
      appendLog(extractResult.summary);
      const extractFinishedAt = performance.now();
      const timing = timingRef.current;
      parseMs = extractFinishedAt - (timing.extractStartedAt ?? extractFinishedAt);

      setSteps((s) =>
        s.map((step, i) =>
          i === 0
            ? { ...step, status: "done", detail: "Extraction complete" }
            : i === 1
              ? { ...step, status: "done", detail: "Attachments processed" }
              : { ...step, status: "active", detail: "Uploading to vault…" },
        ),
      );

      activeStepRef.current = "upload";
      const uploadStartedAt = performance.now();
      pushResult = await awaitTauriJob(
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
            import_id: importSession.id,
          }),
        appendLog,
        applyProgress,
        recordIssue,
      );
      uploadMs = performance.now() - uploadStartedAt;
      appendLog(pushResult.summary);
      if (pushResult.report) {
        countsRef.current.uploadFiles = pushResult.report.conversations_total;
      }
      importCompleted = true;

      setSteps((s) =>
        s.map((step, i) =>
          i === 2 ? { ...step, status: "done", detail: "Upload complete" } : step,
        ),
      );
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      issuesRef.current = [
        ...issuesRef.current,
        { kind: "error", step: activeStepRef.current, item: "Import", reason: msg },
      ];
      setSteps((s) =>
        s.map((step) =>
          step.status === "active" ? { ...step, status: "error" as const } : step,
        ),
      );
      setLog((l) => [...l, `Error: ${msg}`]);
    } finally {
      const durationMs = performance.now() - importStartedAt;
      const finalSummary: ImportSummaryView = {
        status: importCompleted ? "completed" : "failed",
        ...countsRef.current,
        parseMs,
        convertMs,
        uploadMs,
        durationMs,
        issues: issuesRef.current,
      };
      if (importSessionId) {
        try {
          await apiClient.post(`/v1/imports/${String(importSessionId)}/complete`, {
            ok: importCompleted,
            message_count: pushResult?.report?.messages,
            attachment_count: pushResult?.report?.assets_uploaded,
            bytes_uploaded: pushResult?.report?.assets_bytes,
            parse_ms: parseMs,
            convert_ms: convertMs,
            upload_ms: uploadMs,
            duration_ms: durationMs,
            summary: {
              parse_messages: finalSummary.parseMessages,
              convert_detail: finalSummary.convertDetail,
              upload_files: finalSummary.uploadFiles,
            },
            issues: finalSummary.issues,
          });
        } catch (completeError) {
          const msg =
            completeError instanceof Error ? completeError.message : String(completeError);
          appendLog(`Warning: could not complete vault import session ${importSessionId}: ${msg}`);
        }
      }
      setElapsedMs(durationMs);
      setSummaryView(finalSummary);
      setPhase("done");
      setRunning(false);
    }
  };

  return (
    <div className="max-w-[640px] p-6">
      {phase === "form" && (
        <>
          <h1 className="m-0 mb-1 text-2xl font-bold">
            Import Messages
          </h1>
          <p className="m-0 mb-5 text-[0.875rem] text-muted">
            Select your messages.
          </p>

          <CollapsibleSection
            title="Import Messages"
            open={formatOpen}
            onToggle={() => setFormatOpen((o) => !o)}
          >
            <div className={sectionGap}>
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
                  <p className={hintStyle}>{attachmentHelp[attachmentMedia]}</p>
                </StackedField>

                {showCompress && (
                  <div className="mb-[1.1rem] ml-4">
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
                      <p className={hintStyle}>
                        Maximum video resolution; videos are not upscaled.
                      </p>
                    </StackedField>
                    <StackedField label="Max FPS">
                      <input
                        type="text"
                        value={maxFps}
                        onChange={(e) => setMaxFps(e.target.value)}
                        className={fieldStyle}
                      />
                      <p className={hintStyle}>
                        Maximum video frame rate; videos are not upscaled to this FPS.
                      </p>
                    </StackedField>
                    <StackedField label="Minimum Video File Size (Megabytes)">
                      <input
                        type="text"
                        value={minSizeMb}
                        onChange={(e) => setMinSizeMb(e.target.value)}
                        className={fieldStyle}
                      />
                      <p className={hintStyle}>Only re-encode videos above this size.</p>
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
                  className={fieldStyle}
                />
                <p className={hintStyle}>
                  Only conversations with the specified participants are imported, including
                  group conversations.
                </p>
              </StackedField>
              <div className="mb-[1.1rem] flex flex-wrap gap-3">
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
            <label className="mb-3 flex items-center gap-2 text-[0.875rem]">
              <input
                type="checkbox"
                checked={continueOnError}
                onChange={(e) => setContinueOnError(e.target.checked)}
              />
              Continue importing after failed message conversion (default)
            </label>
            <label className="mb-3 flex items-center gap-2 text-[0.875rem]">
              <input
                type="checkbox"
                checked={force}
                onChange={(e) => setForce(e.target.checked)}
              />
              Force reprocessing
            </label>
            {isIos ? (
              <label className="mb-2 flex items-center gap-2 text-[0.875rem]">
                <input
                  type="checkbox"
                  checked={obfuscate}
                  onChange={(e) => setObfuscate(e.target.checked)}
                />
                Obfuscate - All message data is anonymized.
              </label>
            ) : null}
          </CollapsibleSection>

          <div className="mt-2 flex gap-3">
            <Button
              variant="primary"
              onClick={startImport}
              disabled={!backupPath || running}
              className="!rounded-lg !px-6 !py-[0.55rem]"
            >
              Import
            </Button>
          </div>
        </>
      )}

      {(phase === "progress" || phase === "done") && (
        <>
          <h1 className="m-0 mb-4 text-2xl font-bold">
            Import Messages
          </h1>
          <p className="m-0 text-[0.875rem] text-muted">
            Elapsed {formatDuration(elapsedMs)}
          </p>
          <StepProgress steps={steps} />
          <div className="mt-4 flex items-center gap-3">
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
                  setSummaryView(null);
                  setLog([]);
                  setShowDetails(false);
                }}
                className="!px-3 !py-[0.35rem] !text-[0.875rem]"
              >
                ← Back
              </Button>
            )}
            <Button
              variant="ghost"
              onClick={() => setShowDetails(!showDetails)}
              className="!px-2 !py-1 !text-[0.813rem]"
            >
              {showDetails ? "Hide details" : "Show details"}
            </Button>
          </div>
          {showDetails && (
            <pre className="max-h-[300px] overflow-auto whitespace-pre-wrap break-words rounded bg-hover p-2 text-[0.75rem]">
              {log.length === 0
                ? "No log entries"
                : log.map((line, i) => <div key={i}>{line}</div>)}
            </pre>
          )}
        </>
      )}

      {phase === "done" && (
        <>
          {summaryView ? <ImportSummaryPanel summary={summaryView} /> : null}
          <div className="mt-4">
            <Button
              variant="primary"
              onClick={() => {
                setPhase("form");
                setSummaryView(null);
              }}
              className="!px-6 !py-2"
            >
              Import another
            </Button>
          </div>
        </>
      )}
    </div>
  );
}
