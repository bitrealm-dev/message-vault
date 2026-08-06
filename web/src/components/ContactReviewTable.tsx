import type { ContactCard } from "../lib/tauri";

interface ContactReviewTableProps {
  fileCards: ContactCard[];
  onClose: () => void;
}

export default function ContactReviewTable({ fileCards, onClose }: ContactReviewTableProps) {
  return (
    <div style={{ marginTop: "1.5rem" }}>
      <h3 style={{ fontSize: "1rem", marginBottom: "0.75rem" }}>Contacts Found in File</h3>
      <p style={{ fontSize: "0.813rem", color: "#6b7280", marginBottom: "1rem" }}>
        {fileCards.length} contact{fileCards.length !== 1 ? "s" : ""} found. Review before importing.
      </p>
      <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "0.813rem" }}>
        <thead>
          <tr style={{ borderBottom: "2px solid #e5e7eb", textAlign: "left" }}>
            <th style={{ padding: "0.5rem" }}>Name</th>
            <th style={{ padding: "0.5rem" }}>Phone</th>
            <th style={{ padding: "0.5rem" }}>Email</th>
          </tr>
        </thead>
        <tbody>
          {fileCards.map((card, i) => (
            <tr key={i} style={{ borderBottom: "1px solid #f3f4f6" }}>
              <td style={{ padding: "0.5rem" }}>{card.name}</td>
              <td style={{ padding: "0.5rem", color: card.phone ? "#374151" : "#9ca3af" }}>
                {card.phone || "—"}
              </td>
              <td style={{ padding: "0.5rem", color: card.email ? "#374151" : "#9ca3af" }}>
                {card.email || "—"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <div style={{ marginTop: "1rem" }}>
        <button onClick={onClose}
          style={{ padding: "0.375rem 0.75rem", fontSize: "0.875rem" }}>
          Continue
        </button>
      </div>
    </div>
  );
}
