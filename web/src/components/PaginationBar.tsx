import Button from "./Button";

export default function PaginationBar({
  offset,
  limit,
  total,
  onPrev,
  onNext,
}: {
  offset: number;
  limit: number;
  total: number;
  onPrev: () => void;
  onNext: () => void;
}) {
  const start = total === 0 ? 0 : offset + 1;
  const end = Math.min(offset + limit, total);

  return (
    <div style={{
      display: "flex", alignItems: "center", justifyContent: "center",
      gap: "1rem", padding: "0.5rem", borderTop: "1px solid var(--border)",
      fontSize: "0.813rem", color: "var(--muted)",
    }}>
      <Button onClick={onPrev} disabled={offset === 0}
        style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem" }}>
        Previous
      </Button>
      <span>
        Messages {start}–{end} of {total}
      </span>
      <Button onClick={onNext} disabled={offset + limit >= total}
        style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem" }}>
        Next
      </Button>
    </div>
  );
}
