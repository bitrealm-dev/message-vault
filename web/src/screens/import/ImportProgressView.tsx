import Button from "../../components/Button";
import ImportSummaryPanel, {
  type ImportSummaryView,
} from "../../components/import/ImportSummaryPanel";
import OpenPathButton from "../../components/OpenPathButton";
import StepProgress from "../../components/StepProgress";
import type { ImportPhase, ImportStep } from "./useImportJob";
import { PUSH_LOG_NAME } from "./useImportJob";

export default function ImportProgressView({
  phase,
  steps,
  running,
  summaryView,
  stagingDir,
  completionText,
  onCancel,
  onBack,
}: {
  phase: ImportPhase;
  steps: ImportStep[];
  running: boolean;
  summaryView: ImportSummaryView | null;
  stagingDir: string | null;
  completionText?: string;
  onCancel: () => void;
  onBack: () => void;
}) {
  const trimmedStaging = stagingDir?.trim() || null;
  const logPath = trimmedStaging ? `${trimmedStaging}/${PUSH_LOG_NAME}` : null;

  return (
    <>
      <h1 className="m-0 mb-4 text-2xl font-bold">Import Messages</h1>
      {trimmedStaging && logPath ? (
        <div className="mb-4 max-w-[min(36rem,70vw)] text-[0.813rem]">
          <div>
            <span className="text-muted">Staging directory</span>
            <div className="mt-0.5">
              <OpenPathButton
                path={trimmedStaging}
                className="max-w-full truncate border-0 bg-transparent p-0 text-left text-[0.813rem] text-accent underline-offset-2 hover:underline"
              >
                {trimmedStaging}
              </OpenPathButton>
            </div>
          </div>
          <div className="mt-2 border-l border-border pl-3">
            <span className="text-muted">Import log</span>
            <div className="mt-0.5">
              <OpenPathButton
                path={logPath}
                title={logPath}
                className="border-0 bg-transparent p-0 text-left text-[0.813rem] text-accent underline-offset-2 hover:underline"
              >
                {PUSH_LOG_NAME}
              </OpenPathButton>
            </div>
          </div>
        </div>
      ) : null}
      <StepProgress steps={steps} completionText={completionText} />
      <div className="mt-4 flex items-center gap-3">
        {running ? (
          <Button onClick={onCancel}>Cancel</Button>
        ) : (
          <Button variant="ghost" onClick={onBack} className="!px-3 !py-[0.35rem] !text-[0.875rem]">
            ← Back
          </Button>
        )}
      </div>

      {phase === "done" && summaryView ? (
        <>
          <ImportSummaryPanel summary={summaryView} embedStepTimings={false} />
          <div className="mt-4">
            <Button variant="primary" onClick={onBack} size="wide">
              Import another
            </Button>
          </div>
        </>
      ) : null}
    </>
  );
}
