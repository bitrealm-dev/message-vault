import Button from "../../components/Button";
import type { ActiveImportSession } from "../../lib/importSession";
import type { ResumeDecision } from "./resumeDecision";

type ResumableKind = Exclude<ResumeDecision["kind"], "none">;

type PanelCopy = {
  heading: (session: ActiveImportSession) => string;
  body: (session: ActiveImportSession) => string;
  primary: { label: string; action: "resume" | "discard" };
  secondary?: { label: string; action: "discard" };
};

const COPY: Record<ResumableKind, PanelCopy> = {
  resume_push: {
    heading: () => "Finish your last import",
    body: () =>
      "Your messages are staged and ready to upload. Picking up where you left off skips the extract.",
    primary: { label: "Upload to vault", action: "resume" },
    secondary: { label: "Discard this import", action: "discard" },
  },
  restart: {
    heading: () => "Pick up your last import",
    body: () =>
      "The extract did not finish. Starting again reuses your settings and reads the backup from the beginning.",
    primary: { label: "Start over", action: "resume" },
    secondary: { label: "Discard this import", action: "discard" },
  },
  // resumeDecisionFor routes here both when the staged folder has gone
  // missing and when the session never recorded one — every session created
  // outside the desktop app stores a null staging_dir — so the copy names
  // the path only when there is one.
  folder_missing: {
    heading: (session) =>
      session.staging_dir ? "The staged files are gone" : "There is nothing staged to pick up",
    body: (session) =>
      session.staging_dir
        ? `This import's folder is no longer at ${session.staging_dir}. Discarding it lets you start a new one.`
        : "This import did not record a staged folder, so there is nothing here to carry on from. Discarding it lets you start a new one.",
    primary: { label: "Discard this import", action: "discard" },
  },
  other_device: {
    heading: () => "This import belongs to another computer",
    body: () =>
      "It was started on a different install and its files are staged there. Discarding it lets you start a new import here.",
    primary: { label: "Discard this import", action: "discard" },
  },
  settings_unreadable: {
    heading: () => "This import's settings could not be read",
    body: () =>
      "The import is still open here, but the settings it was started with are not readable. Discarding it lets you start a new one.",
    primary: { label: "Discard this import", action: "discard" },
  },
};

/** Renders one resume decision and calls back on the user's choice. */
export default function ResumeImportPanel({
  decision,
  onResume,
  onDiscard,
}: {
  decision: ResumeDecision;
  onResume: () => void;
  onDiscard: () => void;
}) {
  if (decision.kind === "none" || !decision.session) return null;
  const copy = COPY[decision.kind];
  const session = decision.session;

  return (
    <>
      <h1 className="m-0 mb-1 text-2xl font-bold">{copy.heading(session)}</h1>
      <p className="m-0 mb-5 text-[0.875rem] text-muted">{copy.body(session)}</p>
      <div className="flex items-center gap-3">
        <Button
          variant="primary"
          size="wide"
          onClick={copy.primary.action === "resume" ? onResume : onDiscard}
        >
          {copy.primary.label}
        </Button>
        {copy.secondary ? (
          <Button variant="ghost" onClick={onDiscard}>
            {copy.secondary.label}
          </Button>
        ) : null}
      </div>
    </>
  );
}
