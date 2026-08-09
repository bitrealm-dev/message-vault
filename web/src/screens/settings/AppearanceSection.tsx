import { useState, useEffect } from "react";
import { isTauri } from "../../lib/tauri-check";
import FormRow from "../../components/FormRow";
import ThemeSettings from "../../components/ThemeSettings";
import Button from "../../components/Button";

export function AppearanceSection() {
  const [ffmpegPath, setFfmpegPath] = useState("");
  const [saved, setSaved] = useState(false);

  const handleSaveFfmpeg = () => {
    localStorage.setItem("mv-ffmpeg-path", ffmpegPath);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  useEffect(() => {
    setFfmpegPath(localStorage.getItem("mv-ffmpeg-path") || "");
  }, []);

  return (
    <div>
      <ThemeSettings />

      {isTauri() && (
        <div style={{ marginTop: "2rem" }}>
          <h3
            style={{
              fontSize: "12px",
              fontWeight: 600,
              letterSpacing: "0.05em",
              textTransform: "uppercase",
              color: "var(--muted)",
              margin: "0 0 0.5rem",
            }}
          >
            Media
          </h3>
          <FormRow label="ffmpeg path">
            <input
              type="text"
              value={ffmpegPath}
              onChange={(e) => setFfmpegPath(e.target.value)}
              placeholder="Uses system PATH by default"
              style={{
                width: "100%",
                padding: "0.25rem 0.5rem",
                fontSize: "0.875rem",
                border: "1px solid var(--border)",
                borderRadius: "0.375rem",
                background: "var(--bg)",
                color: "var(--text)",
              }}
            />
          </FormRow>
          <p style={{ fontSize: "0.75rem", color: "var(--muted)", marginTop: "0.25rem" }}>
            Leave blank to use system PATH.{" "}
            <a
              href="https://bitrealm-dev.github.io/message-vault-io/ffmpeg"
              target="_blank"
              rel="noopener"
              style={{ color: "var(--accent)" }}
            >
              Install help
            </a>
          </p>
          <div style={{ marginTop: "1rem", display: "flex", alignItems: "center", gap: "0.75rem" }}>
            <Button onClick={handleSaveFfmpeg} style={{ padding: "0.5rem 1.5rem" }}>
              Save
            </Button>
            {saved && (
              <span style={{ fontSize: "0.875rem", color: "var(--accent)" }}>Saved</span>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
