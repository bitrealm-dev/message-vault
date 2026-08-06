import { useState } from "react";
import { invokeContactsInfo, type ContactsInfo } from "../lib/tauri";
import FormRow from "../components/FormRow";
import PathPicker from "../components/PathPicker";

export default function Contacts() {
  const [path, setPath] = useState("");
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<ContactsInfo | null>(null);
  const [error, setError] = useState("");

  const handleParse = async () => {
    setLoading(true);
    setError("");
    setResult(null);
    try {
      const info = await invokeContactsInfo(path);
      setResult(info);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{ padding: "1.5rem", maxWidth: "700px" }}>
      <h2 style={{ margin: "0 0 1.5rem 0" }}>Contacts</h2>

      <FormRow label="Contacts file">
        <PathPicker
          value={path}
          onChange={setPath}
          placeholder="Select .vcf or vCard .csv file"
        />
      </FormRow>

      <div style={{ marginTop: "1.5rem" }}>
        <button
          onClick={handleParse}
          disabled={loading || !path}
          style={{ padding: "0.5rem 1.5rem", fontWeight: 600 }}
        >
          {loading ? "Parsing…" : "Parse Contacts"}
        </button>
      </div>

      {error && (
        <div
          style={{
            marginTop: "1rem",
            padding: "0.75rem 1rem",
            background: "#fef2f2",
            border: "1px solid #fecaca",
            borderRadius: "6px",
            color: "#991b1b",
            fontSize: "0.875rem",
          }}
        >
          {error}
        </div>
      )}

      {result && (
        <div style={{ marginTop: "1.5rem" }}>
          <div
            style={{
              display: "flex",
              gap: "1.5rem",
              marginBottom: "1rem",
              fontSize: "0.875rem",
              color: "#374151",
            }}
          >
            <div>
              <span style={{ fontWeight: 600 }}>Contacts: </span>
              {result.count}
            </div>
            <div>
              <span style={{ fontWeight: 600 }}>Format: </span>
              {result.format.toUpperCase()}
            </div>
          </div>

          {result.preview.length > 0 && (
            <div>
              <div
                style={{
                  fontWeight: 600,
                  fontSize: "0.813rem",
                  color: "#6b7280",
                  marginBottom: "0.5rem",
                }}
              >
                Preview (first {result.preview.length})
              </div>
              <div
                style={{
                  border: "1px solid #e5e7eb",
                  borderRadius: "6px",
                  maxHeight: "300px",
                  overflow: "auto",
                }}
              >
                {result.preview.map((name, i) => (
                  <div
                    key={i}
                    style={{
                      padding: "0.375rem 0.75rem",
                      fontSize: "0.875rem",
                      borderBottom: "1px solid #f3f4f6",
                      background: i % 2 === 0 ? "#fff" : "#f9fafb",
                    }}
                  >
                    {name}
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
