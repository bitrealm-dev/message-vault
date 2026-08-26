import { useEffect, useRef, useState } from "react";
import type { AccountProfile } from "../lib/account";
import { apiClient } from "../lib/api";
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
import {
  getImporterExtraPaths,
  getImporterPath,
  getRememberImporterPaths,
  setImporterExtraPath,
  setImporterPath,
} from "../lib/system-settings";
import { invokeHomeDir, invokeIosBackupEncrypted, invokePathStat } from "../lib/tauri";
import { isTauri } from "../lib/tauri-check";
import type { AttachmentMediaMode, ContactNameMode } from "../lib/types";
import ImportFormFields from "./import/ImportFormFields";
import ImportProgressView from "./import/ImportProgressView";
import { useImportJob } from "./import/useImportJob";

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
  const sourceChangeGenRef = useRef(0);

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
        const profile = await apiClient.get<AccountProfile>("/v1/account/profile");
        if (cancelled) return;
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
    if (!isTauri()) return;
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
        setPathStats(isImessageMethod(source) ? imessageStatsForMethod(source, next) : next);
      })();
    }, PATH_PROBE_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [source, backupPath, attachmentRoot, appleContacts]);

  function applyRememberedPaths(nextSource: string): string {
    const loadedBackup = getImporterPath(nextSource);
    setBackupPath(loadedBackup);
    if (isImessageMethod(nextSource)) {
      const extras = getImporterExtraPaths(nextSource);
      setAttachmentRoot(extras.attachmentRoot);
      setAppleContacts(extras.appleContacts);
    } else {
      setAttachmentRoot("");
      setAppleContacts("");
    }
    return loadedBackup;
  }

  function handleSourceChange(next: string): void {
    const resolved = next === IMESSAGE_SOURCE_ID ? lastImessageMethodRef.current : next;
    const gen = ++sourceChangeGenRef.current;
    setSource(resolved);
    if (isImessageMethod(resolved)) lastImessageMethodRef.current = resolved;
    setPathStats(emptyImessagePathStats());
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

  const isSbr = source === SBR_SOURCE;

  return (
    <div className={`min-w-0 p-6 ${phase === "form" ? "max-w-[640px]" : "max-w-5xl"}`}>
      {phase === "form" && (
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
            })
          }
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
