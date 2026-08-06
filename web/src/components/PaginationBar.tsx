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
      gap: "1rem", padding: "0.5rem", borderTop: "1px solid #e5e7eb",
      fontSize: "0.813rem", color: "#6b7280",
    }}>
      <button onClick={onPrev} disabled={offset === 0}
        style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem" }}>
        Previous
      </button>
      <span>
        Messages {start}–{end} of {total}
      </span>
      <button onClick={onNext} disabled={offset + limit >= total}
        style={{ padding: "0.25rem 0.75rem", fontSize: "0.813rem" }}>
        Next
      </button>
    </div>
  );
}
