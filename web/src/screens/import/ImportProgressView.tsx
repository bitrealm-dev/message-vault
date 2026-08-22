import Button from "../../components/Button";
import ImportSummaryPanel, {
  type ImportSummaryView,
} from "../../components/import/ImportSummaryPanel";
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
  return (
    <>
      <h1 className="m-0 mb-4 text-2xl font-bold">Import Messages</h1>
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
          <ImportSummaryPanel
            summary={summaryView}
            embedStepTimings={false}
            logPath={stagingDir ? `${stagingDir}/${PUSH_LOG_NAME}` : null}
          />
          <div className="mt-4">
            <Button variant="primary" onClick={onBack} className="!px-6 !py-2">
              Import another
            </Button>
          </div>
        </>
      ) : null}
    </>
  );
}
