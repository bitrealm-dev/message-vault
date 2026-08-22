import { useEffect, useState } from "react";
import { getImporterPath, getRememberImporterPaths, setImporterPath } from "../lib/system-settings";
import type { AttachmentMediaMode, ContactNameMode } from "../lib/types";
import ImportFormFields from "./import/ImportFormFields";
import ImportProgressView from "./import/ImportProgressView";
import { useImportJob } from "./import/useImportJob";

const DEFAULT_SOURCE = "imessage-ios";

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
  const [formatOpen, setFormatOpen] = useState(true);
  const [filteringOpen, setFilteringOpen] = useState(false);
  const [processingOpen, setProcessingOpen] = useState(false);
  const [conversationFilter, setConversationFilter] = useState("");
  const [startDate, setStartDate] = useState("");
  const [endDate, setEndDate] = useState("");
  const [force, setForce] = useState(false);
  const [obfuscate, setObfuscate] = useState(false);

  useEffect(() => {
    if (!getRememberImporterPaths()) return;
    setBackupPath(getImporterPath(source));
  }, [source]);

  const updateBackupPath = (path: string) => {
    setBackupPath(path);
    if (getRememberImporterPaths()) setImporterPath(source, path);
  };

  const isIos = source === "imessage-ios";

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
          formatOpen={formatOpen}
          onToggleFormat={() => setFormatOpen((o) => !o)}
          filteringOpen={filteringOpen}
          onToggleFiltering={() => setFilteringOpen((o) => !o)}
          processingOpen={processingOpen}
          onToggleProcessing={() => setProcessingOpen((o) => !o)}
          conversationFilter={conversationFilter}
          onConversationFilterChange={setConversationFilter}
          startDate={startDate}
          onStartDateChange={setStartDate}
          endDate={endDate}
          onEndDateChange={setEndDate}
          force={force}
          onForceChange={setForce}
          obfuscate={obfuscate}
          onObfuscateChange={setObfuscate}
          running={running}
          onImport={() =>
            void startImport({
              source,
              backupPath,
              backupPassword,
              attachmentMedia,
              maxResolution,
              maxFps,
              minSizeMb,
              contactNameMode,
              conversationFilter,
              startDate,
              endDate,
              force,
              obfuscate,
              isIos,
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
