import Button from "../../components/Button";

export default function MessageFindBar({
  findTerm,
  onFindTermChange,
  matchCount,
  activeMatch,
  yearMode,
  onPrevMatch,
  onNextMatch,
}: {
  findTerm: string;
  onFindTermChange: (value: string) => void;
  matchCount: number;
  activeMatch: number;
  yearMode: boolean;
  onPrevMatch: () => void;
  onNextMatch: () => void;
}) {
  return (
    <div className="flex items-center gap-2 border-b border-border px-6 py-1.5">
      <input
        type="text"
        value={findTerm}
        onChange={(e) => onFindTermChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            if (matchCount > 0) onNextMatch();
          }
        }}
        placeholder="Find in conversation…"
        className="box-border flex-1 rounded border border-border bg-bg px-2 py-1 text-[0.813rem] text-text"
      />
      {matchCount > 0 && (
        <>
          <span className="whitespace-nowrap text-[0.75rem] text-muted">
            {activeMatch + 1} of {matchCount}
            {yearMode ? " in this year" : " on this page"}
          </span>
          <Button onClick={onPrevMatch} className="!px-1.5 !py-1 !text-[0.813rem]">
            ↑
          </Button>
          <Button onClick={onNextMatch} className="!px-1.5 !py-1 !text-[0.813rem]">
            ↓
          </Button>
        </>
      )}
    </div>
  );
}
