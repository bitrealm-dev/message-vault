import { useEffect, useRef, useState } from "react";
import { getDeviceId } from "../lib/deviceId";
import {
  emptyImessagePathStats,
  IMESSAGE_DEFAULT_METHOD,
  IMESSAGE_SOURCE_ID,
  type ImessageMethodId,
  imessageStatsForMethod,
  isImessageMethod,
  macMessagesDbPath,
  type PathStat,
  shouldPrefillMacMessagesDb,
} from "../lib/imessageImport";
import { discardImportSession, getActiveImportSession } from "../lib/importSession";
import {
  getImporterPath,
  getRememberImporterPaths,
  loadRememberedImportPaths,
  setImporterExtraPath,
  setImporterPath,
} from "../lib/system-settings";
import { invokeHomeDir, invokeIosBackupEncrypted, invokePathStat } from "../lib/tauri";
import { isTauri } from "../lib/tauri-check";
import type { AttachmentMediaMode, ContactNameMode } from "../lib/types";
import { loadAccountProfile } from "../lib/useAccountProfile";
import {
  emptyWhatsappPathStats,
  isWhatsappMethod,
  WHATSAPP_CRYPT_NAMES,
  WHATSAPP_DEFAULT_METHOD,
  WHATSAPP_SOURCE_ID,
  type WhatsappMethodId,
} from "../lib/whatsappImport";
import ImportFormFields from "./import/ImportFormFields";
import ImportProgressView from "./import/ImportProgressView";
import ResumeImportPanel from "./import/ResumeImportPanel";
import { type ResumeDecision, resumeDecisionFor } from "./import/resumeDecision";
import { restoreFormFromSnapshot, useImportJob } from "./import/useImportJob";

const DEFAULT_SOURCE = IMESSAGE_DEFAULT_METHOD;
const SBR_SOURCE = "sms-backup-restore";
const PATH_PROBE_DEBOUNCE_MS = 200;

function mapPathStat(raw: { exists: boolean; isFile: boolean; isDirectory: boolean }): PathStat {
  return {
    exists: raw.exists,
    isFile: raw.isFile,
    isDirectory: raw.isDirectory,
  };
}

async function probePath(path: string): Promise<PathStat | null> {
  const trimmed = path.trim();
  if (trimmed === "") return null;
  try {
    return mapPathStat(await invokePathStat(trimmed));
  } catch {
    return { exists: false, isFile: false, isDirectory: false };
  }
}

export default function ImportScreen() {
  const {
    phase,
    steps,
    running,
    summaryView,
    stagingDir,
    completionText,
    startImport,
    cancel,
    returnToForm,
  } = useImportJob();

  const [source, setSource] = useState(DEFAULT_SOURCE);
  const [backupPath, setBackupPath] = useState(() =>
    getRememberImporterPaths() ? getImporterPath(DEFAULT_SOURCE) : "",
  );
  const [attachmentRoot, setAttachmentRoot] = useState("");
  const [appleContacts, setAppleContacts] = useState("");
  const [pathStats, setPathStats] = useState(emptyImessagePathStats);
  const [whatsappKey, setWhatsappKey] = useState("");
  const [showWhatsappKey, setShowWhatsappKey] = useState(false);
  const [whatsappWa, setWhatsappWa] = useState("");
  const [whatsappMedia, setWhatsappMedia] = useState("");
  const [whatsappDb, setWhatsappDb] = useState("");
  const [whatsappBusiness, setWhatsappBusiness] = useState(false);
  const [whatsappStats, setWhatsappStats] = useState(emptyWhatsappPathStats);
  const [backupPassword, setBackupPassword] = useState("");
  const [showBackupPassword, setShowBackupPassword] = useState(false);
  const [attachmentMedia, setAttachmentMedia] = useState<AttachmentMediaMode>("copy");
  const [maxResolution, setMaxResolution] = useState("720p");
  const [maxFps, setMaxFps] = useState("30");
  const [minSizeMb, setMinSizeMb] = useState("20");
  const [contactNameMode, setContactNameMode] = useState<ContactNameMode>("fill_missing");
  const [ownerPhones, setOwnerPhones] = useState<string[]>([]);
  const [formatOpen, setFormatOpen] = useState(true);
  const [processingOpen, setProcessingOpen] = useState(false);
  const [force, setForce] = useState(false);
  const [obfuscate, setObfuscate] = useState(false);
  /** Profile phones after SBR fetch; empty until ready (or after a failed fetch). */
  const [profilePhones, setProfilePhones] = useState<string[]>([]);
  const [profilePhonesReady, setProfilePhonesReady] = useState(false);
  const [profilePhonesError, setProfilePhonesError] = useState(false);
  const ownerPhonesSeededRef = useRef(false);
  const lastImessageMethodRef = useRef<ImessageMethodId>(IMESSAGE_DEFAULT_METHOD);
  const lastWhatsappMethodRef = useRef<WhatsappMethodId>(WHATSAPP_DEFAULT_METHOD);
  const sourceChangeGenRef = useRef(0);

  const [resume, setResume] = useState<ResumeDecision | null>(null);
  const [resumeChecked, setResumeChecked] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const session = await getActiveImportSession();
        const folderExists = session?.staging_dir
          ? ((await probePath(session.staging_dir))?.exists ?? false)
          : false;
        if (!cancelled) {
          setResume(resumeDecisionFor({ session, deviceId: getDeviceId(), folderExists }));
        }
      } catch {
        // A vault that cannot answer is not a reason to block the form.
        if (!cancelled) setResume(null);
      } finally {
        if (!cancelled) setResumeChecked(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  /** Populate the visible form from a resumed or restarted session's settings. */
  function applyRestoredFormState(restored: ReturnType<typeof restoreFormFromSnapshot>): void {
    if (!restored) return;
    setSource(restored.source);
    setBackupPath(restored.backupPath);
    setBackupPassword("");
    setAttachmentRoot(restored.attachmentRoot);
    setAppleContacts(restored.appleContacts);
    setWhatsappKey("");
    setWhatsappWa(restored.whatsappWa);
    setWhatsappMedia(restored.whatsappMedia);
    setWhatsappDb(restored.whatsappDb);
    setWhatsappBusiness(restored.whatsappBusiness);
    setAttachmentMedia(restored.attachmentMedia);
    setMaxResolution(restored.maxResolution);
    setMaxFps(restored.maxFps);
    setMinSizeMb(restored.minSizeMb);
    setContactNameMode(restored.contactNameMode);
    setOwnerPhones(restored.ownerPhones);
    setForce(restored.force);
    setObfuscate(restored.obfuscate);
  }

  async function handleDiscardResume(): Promise<void> {
    const session = resume?.session;
    if (!session) return;
    try {
      await discardImportSession(session.id);
    } catch {
      // Best effort -- the panel drops to the form either way; if the
      // session is still live server-side, the next visit shows it again.
    } finally {
      setResume({ kind: "none", canResume: false, session: null });
    }
  }

  async function handleResumeAction(): Promise<void> {
    if (!resume || resume.kind === "none" || !resume.session) return;
    const session = resume.session;
    const restoredForm = restoreFormFromSnapshot(session.form);
    if (!restoredForm) {
      // A stored snapshot the vault can't be trusted to have kept valid --
      // there is nothing safe to resume or restart with, so fall back to
      // the same discard-only handling as a missing staging folder.
      setResume({ kind: "folder_missing", canResume: false, session });
      return;
    }
    applyRestoredFormState(restoredForm);

    if (resume.kind === "resume_push" && session.staging_dir) {
      setResume({ kind: "none", canResume: false, session: null });
      await startImport(restoredForm, { sessionId: session.id, stagingDir: session.staging_dir });
      return;
    }

    // Restart: a fresh extract writes into a new staging folder, and the
    // vault allows only one live session per account, so give up the old
    // one before starting the new run.
    setResume({ kind: "none", canResume: false, session: null });
    try {
      await discardImportSession(session.id);
    } catch {
      // Best effort -- if the vault is unreachable the create call below
      // surfaces its own error the same as any other failed import start.
    }
    await startImport(restoredForm);
  }

  useEffect(() => {
    if (source !== SBR_SOURCE) {
      setProfilePhones([]);
      setProfilePhonesReady(false);
      setProfilePhonesError(false);
      ownerPhonesSeededRef.current = false;
      return;
    }
    let cancelled = false;
    setProfilePhonesReady(false);
    setProfilePhonesError(false);
    void (async () => {
      try {
        const profile = await loadAccountProfile();
        if (cancelled) return;
        if (!profile) throw new Error("profile unavailable");
        setProfilePhones([...profile.phones]);
        setProfilePhonesError(false);
        setProfilePhonesReady(true);
        if (profile.phones.length === 0 || ownerPhonesSeededRef.current) return;
        setOwnerPhones((current) => {
          if (current.length > 0) return current;
          ownerPhonesSeededRef.current = true;
          return [...profile.phones];
        });
      } catch {
        if (!cancelled) {
          setProfilePhones([]);
          setProfilePhonesError(true);
          setProfilePhonesReady(true);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [source]);

  useEffect(() => {
    return () => {
      sourceChangeGenRef.current += 1;
    };
  }, []);

  useEffect(() => {
    if (!isTauri() || !isImessageMethod(source)) return;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void (async () => {
        const [backup, attachment, contacts] = await Promise.all([
          probePath(backupPath),
          probePath(attachmentRoot),
          probePath(appleContacts),
        ]);
        let backupEncrypted: boolean | null = null;
        if (source === "imessage-ios" && backup?.exists && backup.isDirectory) {
          try {
            backupEncrypted = await invokeIosBackupEncrypted(backupPath.trim());
          } catch {
            backupEncrypted = null;
          }
        }
        if (cancelled) return;
        const next = {
          backup,
          attachmentRoot: attachment,
          appleContacts: contacts,
          backupEncrypted,
        };
        setPathStats(imessageStatsForMethod(source, next));
      })();
    }, PATH_PROBE_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [source, backupPath, attachmentRoot, appleContacts]);

  useEffect(() => {
    if (!isTauri() || !isWhatsappMethod(source)) return;
    setWhatsappStats(emptyWhatsappPathStats());
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void (async () => {
        const root = backupPath.trim();
        const [backup, contactsDb, media, db, msgstore, cryptHits] = await Promise.all([
          probePath(backupPath),
          probePath(whatsappWa),
          probePath(whatsappMedia),
          probePath(whatsappDb),
          probePath(root ? `${root}/msgstore.db` : ""),
          Promise.all(
            WHATSAPP_CRYPT_NAMES.map(async (name) => {
              const stat = await probePath(root ? `${root}/${name}` : "");
              return stat?.exists && stat.isFile ? name : null;
            }),
          ),
        ]);
        if (cancelled) return;
        const cryptName = cryptHits.find((name) => name !== null) ?? null;
        setWhatsappStats({
          backup,
          contactsDb,
          media,
          db,
          hasMsgstoreDb: Boolean(msgstore?.exists && msgstore.isFile),
          cryptName,
        });
      })();
    }, PATH_PROBE_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [source, backupPath, whatsappWa, whatsappMedia, whatsappDb]);

  function applyRememberedPaths(nextSource: string): string {
    const loaded = loadRememberedImportPaths(nextSource);
    setBackupPath(loaded.backupPath);
    if (isImessageMethod(nextSource)) {
      setAttachmentRoot(loaded.attachmentRoot);
      setAppleContacts(loaded.appleContacts);
      setWhatsappWa("");
      setWhatsappMedia("");
      setWhatsappDb("");
    } else if (isWhatsappMethod(nextSource)) {
      setAttachmentRoot("");
      setAppleContacts("");
      setWhatsappWa(loaded.whatsappWa);
      setWhatsappMedia(loaded.whatsappMedia);
      setWhatsappDb(loaded.whatsappDb);
    } else {
      setAttachmentRoot("");
      setAppleContacts("");
      setWhatsappWa("");
      setWhatsappMedia("");
      setWhatsappDb("");
    }
    return loaded.backupPath;
  }

  function handleSourceChange(next: string): void {
    const resolved =
      next === IMESSAGE_SOURCE_ID
        ? lastImessageMethodRef.current
        : next === WHATSAPP_SOURCE_ID
          ? lastWhatsappMethodRef.current
          : next;
    const gen = ++sourceChangeGenRef.current;
    setSource(resolved);
    if (isImessageMethod(resolved)) lastImessageMethodRef.current = resolved;
    if (isWhatsappMethod(resolved)) lastWhatsappMethodRef.current = resolved;
    setPathStats(emptyImessagePathStats());
    setWhatsappStats(emptyWhatsappPathStats());
    if (resolved !== "whatsapp-ios") setWhatsappBusiness(false);
    const loadedBackup = applyRememberedPaths(resolved);

    if (resolved !== "imessage-macos" || loadedBackup.trim() !== "" || !isTauri()) {
      return;
    }

    void (async () => {
      try {
        const home = await invokeHomeDir();
        if (gen !== sourceChangeGenRef.current) return;
        if (home.os !== "macos") return;
        const chatDb = macMessagesDbPath(home.path);
        if (chatDb === "") return;
        const stat = mapPathStat(await invokePathStat(chatDb));
        if (gen !== sourceChangeGenRef.current) return;
        const prefill = shouldPrefillMacMessagesDb({
          os: home.os,
          homeDir: home.path,
          chatDbExists: stat.exists && stat.isFile,
          rememberedPath: loadedBackup,
        });
        if (prefill === "") return;
        setBackupPath(prefill);
        if (getRememberImporterPaths()) setImporterPath(resolved, prefill);
      } catch {
        // Home directory and path checks are best-effort on Mac only.
      }
    })();
  }

  const updateBackupPath = (path: string) => {
    setBackupPath(path);
    if (getRememberImporterPaths()) setImporterPath(source, path);
  };

  const updateAttachmentRoot = (path: string) => {
    setAttachmentRoot(path);
    if (getRememberImporterPaths() && isImessageMethod(source)) {
      setImporterExtraPath(source, "attachmentRoot", path);
    }
  };

  const updateAppleContacts = (path: string) => {
    setAppleContacts(path);
    if (getRememberImporterPaths() && isImessageMethod(source)) {
      setImporterExtraPath(source, "appleContacts", path);
    }
  };

  const updateWhatsappWa = (path: string) => {
    setWhatsappWa(path);
    if (getRememberImporterPaths() && isWhatsappMethod(source)) {
      setImporterExtraPath(source, "whatsappWa", path);
    }
  };

  const updateWhatsappMedia = (path: string) => {
    setWhatsappMedia(path);
    if (getRememberImporterPaths() && isWhatsappMethod(source)) {
      setImporterExtraPath(source, "whatsappMedia", path);
    }
  };

  const updateWhatsappDb = (path: string) => {
    setWhatsappDb(path);
    if (getRememberImporterPaths() && isWhatsappMethod(source)) {
      setImporterExtraPath(source, "whatsappDb", path);
    }
  };

  const isSbr = source === SBR_SOURCE;

  return (
    <div className={`min-w-0 p-6 ${phase === "form" ? "max-w-[640px]" : "max-w-5xl"}`}>
      {phase === "form" && resumeChecked && (!resume || resume.kind === "none") && (
        <ImportFormFields
          source={source}
          onSourceChange={handleSourceChange}
          backupPath={backupPath}
          onBackupPathChange={updateBackupPath}
          backupPassword={backupPassword}
          onBackupPasswordChange={setBackupPassword}
          showBackupPassword={showBackupPassword}
          onToggleBackupPassword={() => setShowBackupPassword((v) => !v)}
          attachmentRoot={attachmentRoot}
          onAttachmentRootChange={updateAttachmentRoot}
          appleContacts={appleContacts}
          onAppleContactsChange={updateAppleContacts}
          pathStats={pathStats}
          whatsappKey={whatsappKey}
          onWhatsappKeyChange={setWhatsappKey}
          showWhatsappKey={showWhatsappKey}
          onToggleWhatsappKey={() => setShowWhatsappKey((v) => !v)}
          whatsappWa={whatsappWa}
          onWhatsappWaChange={updateWhatsappWa}
          whatsappMedia={whatsappMedia}
          onWhatsappMediaChange={updateWhatsappMedia}
          whatsappDb={whatsappDb}
          onWhatsappDbChange={updateWhatsappDb}
          whatsappBusiness={whatsappBusiness}
          onWhatsappBusinessChange={setWhatsappBusiness}
          whatsappStats={whatsappStats}
          attachmentMedia={attachmentMedia}
          onAttachmentMediaChange={setAttachmentMedia}
          maxResolution={maxResolution}
          onMaxResolutionChange={setMaxResolution}
          maxFps={maxFps}
          onMaxFpsChange={setMaxFps}
          minSizeMb={minSizeMb}
          onMinSizeMbChange={setMinSizeMb}
          contactNameMode={contactNameMode}
          onContactNameModeChange={setContactNameMode}
          ownerPhones={ownerPhones}
          onOwnerPhonesChange={(phones) => {
            ownerPhonesSeededRef.current = true;
            setOwnerPhones(phones);
          }}
          profilePhones={profilePhones}
          profilePhonesReady={profilePhonesReady}
          profilePhonesError={profilePhonesError}
          showMissingAccountPhoneWarning={
            profilePhonesReady && !profilePhonesError && profilePhones.length === 0
          }
          formatOpen={formatOpen}
          onToggleFormat={() => setFormatOpen((o) => !o)}
          processingOpen={processingOpen}
          onToggleProcessing={() => setProcessingOpen((o) => !o)}
          force={force}
          onForceChange={setForce}
          obfuscate={obfuscate}
          onObfuscateChange={setObfuscate}
          running={running}
          onImport={(flushedPhones) =>
            void startImport({
              source,
              backupPath,
              backupPassword,
              attachmentMedia,
              maxResolution,
              maxFps,
              minSizeMb,
              contactNameMode,
              ownerPhones: flushedPhones ?? ownerPhones,
              force,
              obfuscate,
              isSbr,
              attachmentRoot,
              appleContacts,
              whatsappKey,
              whatsappWa,
              whatsappMedia,
              whatsappDb,
              whatsappBusiness,
            })
          }
        />
      )}

      {phase === "form" && resumeChecked && resume && resume.kind !== "none" && (
        <ResumeImportPanel
          decision={resume}
          onResume={() => void handleResumeAction()}
          onDiscard={() => void handleDiscardResume()}
        />
      )}

      {(phase === "progress" || phase === "done") && (
        <ImportProgressView
          phase={phase}
          steps={steps}
          running={running}
          summaryView={summaryView}
          stagingDir={stagingDir}
          completionText={completionText}
          onCancel={() => void cancel()}
          onBack={returnToForm}
        />
      )}
    </div>
  );
}
