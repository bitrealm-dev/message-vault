import { useEffect, useRef, useState } from "react";
import type { AccountProfile } from "../lib/account";
import { apiClient } from "../lib/api";
import { getImporterPath, getRememberImporterPaths, setImporterPath } from "../lib/system-settings";
import type { AttachmentMediaMode, ContactNameMode } from "../lib/types";
import ImportFormFields from "./import/ImportFormFields";
import ImportProgressView from "./import/ImportProgressView";
import { useImportJob } from "./import/useImportJob";

const DEFAULT_SOURCE = "imessage-ios";
const SBR_SOURCE = "sms-backup-restore";

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

  useEffect(() => {
    if (!getRememberImporterPaths()) return;
    setBackupPath(getImporterPath(source));
  }, [source]);

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

  const updateBackupPath = (path: string) => {
    setBackupPath(path);
    if (getRememberImporterPaths()) setImporterPath(source, path);
  };

  const isIos = source === "imessage-ios";
  const isSbr = source === SBR_SOURCE;

  return (
    <div className={`min-w-0 p-6 ${phase === "form" ? "max-w-[640px]" : "max-w-5xl"}`}>
      {phase === "form" && (
        <ImportFormFields
          source={source}
          onSourceChange={setSource}
          backupPath={backupPath}
          onBackupPathChange={updateBackupPath}
          backupPassword={backupPassword}
          onBackupPasswordChange={setBackupPassword}
          showBackupPassword={showBackupPassword}
          onToggleBackupPassword={() => setShowBackupPassword((v) => !v)}
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
              isIos,
              isSbr,
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
