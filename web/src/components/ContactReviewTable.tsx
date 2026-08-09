import type { ContactCard } from "../lib/tauri";
import Button from "./Button";

interface ContactReviewTableProps {
  fileCards: ContactCard[];
  onClose: () => void;
}

export default function ContactReviewTable({ fileCards, onClose }: ContactReviewTableProps) {
  return (
    <div style={{ marginTop: "1.5rem" }}>
      <h3 style={{ fontSize: "1rem", marginBottom: "0.75rem" }}>Contacts Found in File</h3>
      <p style={{ fontSize: "0.813rem", color: "var(--muted)", marginBottom: "1rem" }}>
        {fileCards.length} contact{fileCards.length !== 1 ? "s" : ""} found. Review before importing.
      </p>
      <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "0.813rem" }}>
        <thead>
          <tr style={{ borderBottom: "2px solid var(--border)", textAlign: "left" }}>
            <th style={{ padding: "0.5rem" }}>Name</th>
            <th style={{ padding: "0.5rem" }}>Phone</th>
            <th style={{ padding: "0.5rem" }}>Email</th>
          </tr>
        </thead>
        <tbody>
          {fileCards.map((card, i) => (
            <tr key={i} style={{ borderBottom: "1px solid var(--border)" }}>
              <td style={{ padding: "0.5rem" }}>{card.name}</td>
              <td style={{ padding: "0.5rem", color: card.phone ? "var(--text)" : "var(--muted)" }}>
                {card.phone || "—"}
              </td>
              <td style={{ padding: "0.5rem", color: card.email ? "var(--text)" : "var(--muted)" }}>
                {card.email || "—"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <div style={{ marginTop: "1rem" }}>
        <Button variant="primary" onClick={onClose} style={{ padding: "0.375rem 0.75rem" }}>
          Continue
        </Button>
      </div>
    </div>
  );
}
