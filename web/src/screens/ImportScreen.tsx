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

type ImportStep = {
  label: string;
  status: "pending" | "active" | "done" | "error";
  detail?: string;
  pathLink?: string;
  durationMs?: number | null;
};

type ImportPhase = "form" | "progress" | "done";

const DEFAULT_SOURCE = "imessage-ios";
const PUSH_LOG_NAME = "vault-push.log";

const EMPTY_TIMING = {
  extractStartedAt: null as number | null,
  parseStartedAt: null as number | null,
  parseEndedAt: null as number | null,
  convertStartedAt: null as number | null,
  convertEndedAt: null as number | null,
};

function initialSteps(status: ImportStep["status"] = "pending"): ImportStep[] {
  return [
    { label: "Parse backup", status, detail: status === "active" ? "Parsing backup…" : undefined },
    { label: "Convert attachments", status: "pending" },
    { label: "Upload to vault", status: "pending" },
  ];
}

function stepIndexFor(step: ImportProgressEvent["step"]): number {
  if (step === "parse") return 0;
  if (step === "convert") return 1;
  return 2;
}

function progressVerb(step: ImportProgressEvent["step"]): string {
  if (step === "upload") return "Uploading";
  if (step === "convert") return "Converting";
  return "Parsing";
}

function completionTextFor(
  status: ImportSummaryView["status"] | undefined,
): string | undefined {
  if (status === "completed") return "Import complete";
  if (status === "canceled") return "Import canceled";
  if (status === "failed") return "Import failed";
  return undefined;
}

function stageDurations(
  timing: typeof EMPTY_TIMING,
  extractFinishedAt: number,
): { parseMs: number; convertMs: number } {
  const parseStart =
    timing.parseStartedAt ?? timing.extractStartedAt ?? extractFinishedAt;
  if (timing.convertStartedAt != null) {
    return {
      parseMs: Math.max(0, (timing.parseEndedAt ?? timing.convertStartedAt) - parseStart),
      convertMs: Math.max(0, extractFinishedAt - timing.convertStartedAt),
    };
  }
  return {
    parseMs: Math.max(0, extractFinishedAt - (timing.extractStartedAt ?? parseStart)),
    convertMs: 0,
  };
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
  const [force, setForce] = useState(false);
  const [obfuscate, setObfuscate] = useState(false);

  const [running, setRunning] = useState(false);
  const [steps, setSteps] = useState<ImportStep[]>(() => initialSteps());
  const [phase, setPhase] = useState<ImportPhase>("form");
  const [summaryView, setSummaryView] = useState<ImportSummaryView | null>(null);
  const [stagingDir, setStagingDir] = useState<string | null>(null);
  const activeStepRef = useRef<ImportIssue["step"]>("parse");
  const issuesRef = useRef<ImportIssue[]>([]);
  const countsRef = useRef<{
    filesParsed?: number;
    messagesParsed?: number;
  }>({});
  const timingRef = useRef({ ...EMPTY_TIMING });

  function returnToForm(): void {
    setPhase("form");
    setSummaryView(null);
    setStagingDir(null);
  }

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

  function applyProgress(event: ImportProgressEvent): void {
    const now = performance.now();
    activeStepRef.current = event.step;

    if (event.step === "parse") {
      timingRef.current.parseStartedAt ??= now;
      countsRef.current.messagesParsed =
        event.total > 0 && event.done >= event.total ? event.total : event.done;
    } else if (event.step === "convert" && timingRef.current.convertStartedAt == null) {
      timingRef.current.parseEndedAt ??= now;
      timingRef.current.convertStartedAt = now;
    }

    const stepIndex = stepIndexFor(event.step);
    let rawDetail = `${event.done}/${event.total}`;
    if (event.status === "included_in_extract") {
      rawDetail = "Included in extract";
    } else if (event.status) {
      rawDetail = `${event.done}/${event.total} (${event.status})`;
    }

    const detail =
      event.status === "included_in_extract" && event.step === "convert"
        ? rawDetail
        : `${progressVerb(event.step)} ${rawDetail}`;
    const done = event.total > 0 && event.done >= event.total;

    setSteps((current) =>
      current.map((step, index) => {
        if (index < stepIndex) {
          return { ...step, status: "done" };
        }
        if (index > stepIndex) return step;
        return {
          ...step,
          status: done ? "done" : "active",
          detail,
        };
      }),
    );
  }

  function recordIssue(issue: ImportIssueEvent): void {
    issuesRef.current = [...issuesRef.current, issue];
  }

  async function startImport(): Promise<void> {
    if (!isTauri()) return;
    const importStartedAt = performance.now();
    activeStepRef.current = "parse";
    issuesRef.current = [];
    countsRef.current = {};
    timingRef.current = { ...EMPTY_TIMING };
    setRunning(true);
    setPhase("progress");
    setSummaryView(null);
    setStagingDir(null);
    setSteps(initialSteps("active"));

    let importSessionId: number | null = null;
    let importCompleted = false;
    let parseMs: number | null = null;
    let convertMs: number | null = null;
    let uploadMs: number | null = null;
    let pushResult: TauriJobResult | null = null;
    let outputDir = "";
    try {
      const baseUrl = getBaseUrl();
      if (!token) throw new Error("Not authenticated");

      const importSession = await apiClient.post<{ id: number }>("/v1/imports", {
        source,
        tool: "message-vault-io",
        mode: "append",
      });
      importSessionId = importSession.id;

      outputDir = await resolveImportStagingDir(backupPath, source);
      setStagingDir(outputDir);
      setSteps((current) =>
        current.map((step, i) =>
          i === 0 ? { ...step, pathLink: outputDir, detail: "Extracting…" } : step,
        ),
      );

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
        undefined,
        applyProgress,
        recordIssue,
      );
      if (extractResult.extraction) {
        countsRef.current.filesParsed = extractResult.extraction.files_parsed;
        countsRef.current.messagesParsed = extractResult.extraction.messages_parsed;
      }

      const extractFinishedAt = performance.now();
      timingRef.current.convertEndedAt = extractFinishedAt;
      ({ parseMs, convertMs } = stageDurations(timingRef.current, extractFinishedAt));

      setSteps([
        {
          label: "Parse backup",
          status: "done",
          pathLink: outputDir,
          detail: "Extraction complete",
          durationMs: parseMs,
        },
        {
          label: "Convert attachments",
          status: "done",
          detail: "Attachments processed",
          durationMs: convertMs,
        },
        {
          label: "Upload to vault",
          status: "active",
          detail: "Uploading to vault…",
        },
      ]);

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
            continue_on_error: true,
            skip_attachments: false,
            trust_export: false,
            contact_name_mode: contactNameMode,
            import_id: importSession.id,
          }),
        undefined,
        applyProgress,
        recordIssue,
      );
      uploadMs = performance.now() - uploadStartedAt;
      importCompleted = true;

      setSteps((current) =>
        current.map((step, i) =>
          i === 2
            ? { ...step, status: "done", detail: "Upload complete", durationMs: uploadMs }
            : step,
        ),
      );
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      issuesRef.current = [
        ...issuesRef.current,
        { kind: "error", step: activeStepRef.current, item: "Import", reason: msg },
      ];
      setSteps((current) =>
        current.map((step) =>
          step.status === "active" ? { ...step, status: "error" as const } : step,
        ),
      );
    } finally {
      const durationMs = performance.now() - importStartedAt;
      const pushReport = pushResult?.report;
      const finalSummary: ImportSummaryView = {
        status: importCompleted ? "completed" : "failed",
        ...countsRef.current,
        filesTotal: pushReport?.conversations_total ?? countsRef.current.filesParsed,
        filesSucceeded: pushReport?.conversations_ok,
        filesFailed: pushReport?.conversations_failed,
        filesSkipped: pushReport?.conversations_skipped,
        messagesAttempted: pushReport?.messages_attempted,
        messagesInserted: pushReport?.messages_inserted,
        messagesDeduped: pushReport?.messages_deduped,
        messagesFailed: pushReport?.messages_failed,
        parseMs,
        convertMs,
        uploadMs,
        durationMs,
        issues: issuesRef.current,
      };
      const durations = [parseMs, convertMs, uploadMs];
      setSteps((current) =>
        current.map((step, index) => {
          const duration = durations[index];
          if (duration == null) return step;
          return { ...step, durationMs: duration };
        }),
      );
      if (importSessionId) {
        try {
          await apiClient.post(`/v1/imports/${String(importSessionId)}/complete`, {
            ok: importCompleted,
            message_count: pushReport?.messages_inserted,
            attachment_count: pushReport?.assets_uploaded,
            bytes_uploaded: pushReport?.assets_bytes,
            parse_ms: parseMs,
            convert_ms: convertMs,
            upload_ms: uploadMs,
            duration_ms: durationMs,
            summary: {
              files_total: finalSummary.filesTotal,
              files_succeeded: finalSummary.filesSucceeded,
              files_failed: finalSummary.filesFailed,
              files_skipped: finalSummary.filesSkipped,
              messages_parsed: finalSummary.messagesParsed,
              messages_attempted: finalSummary.messagesAttempted,
              messages_inserted: finalSummary.messagesInserted,
              messages_deduped: finalSummary.messagesDeduped,
              messages_failed: finalSummary.messagesFailed,
            },
            issues: finalSummary.issues,
          });
        } catch {
          // Session complete is best-effort; summary UI still shows local results.
        }
      }
      setSummaryView(finalSummary);
      setPhase("done");
      setRunning(false);
    }
  }

  return (
    <div className={`min-w-0 p-6 ${phase === "form" ? "max-w-[640px]" : "max-w-5xl"}`}>
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
                    aria-label="Encryption password"
                    value={backupPassword}
                    onChange={setBackupPassword}
                    autoComplete="new-password"
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
                    <ListBoxItem id="as_is" className={selectItemClassName}>
                      Leave unknown names as is
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
          <h1 className="m-0 mb-4 text-2xl font-bold">Import Messages</h1>
          <StepProgress
            steps={steps}
            completionText={
              phase === "done" ? completionTextFor(summaryView?.status) : undefined
            }
          />
          <div className="mt-4 flex items-center gap-3">
            {running ? (
              <Button onClick={() => invokeCancel()}>Cancel</Button>
            ) : (
              <Button
                variant="ghost"
                onClick={returnToForm}
                className="!px-3 !py-[0.35rem] !text-[0.875rem]"
              >
                ← Back
              </Button>
            )}
          </div>
        </>
      )}

      {phase === "done" && summaryView ? (
        <>
          <ImportSummaryPanel
            summary={summaryView}
            embedStepTimings={false}
            logPath={stagingDir ? `${stagingDir}/${PUSH_LOG_NAME}` : null}
          />
          <div className="mt-4">
            <Button
              variant="primary"
              onClick={returnToForm}
              className="!px-6 !py-2"
            >
              Import another
            </Button>
          </div>
        </>
      ) : null}
    </div>
  );
}
