import { useState, useEffect, type CSSProperties } from "react";
import { isTauri } from "../lib/tauri-check";
import { apiClient } from "../lib/api";
import FormRow from "../components/FormRow";
import ThemeSettings from "../components/ThemeSettings";
import Button from "../components/Button";
import { ProfileSettingsPanel } from "./ProfileScreen";

type SettingsTab = "profile" | "storage" | "appearance";

const TABS: { id: SettingsTab; label: string }[] = [
  { id: "profile", label: "Profile" },
  { id: "storage", label: "Storage" },
  { id: "appearance", label: "Appearance" },
];

export default function SettingsScreen() {
  const [tab, setTab] = useState<SettingsTab>("profile");

  return (
    <div style={{ padding: "1.5rem", maxWidth: "820px", color: "var(--text)" }}>
      <header style={{ marginBottom: "1.5rem" }}>
        <h2 style={{ margin: 0, color: "var(--text)" }}>Settings</h2>
        <p style={{ margin: "0.35rem 0 0", fontSize: "0.875rem", color: "var(--muted)" }}>
          Manage your profile, storage, and appearance.
        </p>
        <nav
          aria-label="Settings sections"
          style={{
            display: "flex",
            gap: "0.25rem",
            marginTop: "1.25rem",
            borderBottom: "1px solid var(--border)",
          }}
        >
          {TABS.map((t) => {
            const active = tab === t.id;
            return (
              <button
                key={t.id}
                type="button"
                onClick={() => setTab(t.id)}
                style={{
                  position: "relative",
                  padding: "0.5rem 0.75rem",
                  fontSize: "0.813rem",
                  fontWeight: 500,
                  color: active ? "var(--text)" : "var(--muted)",
                  background: "transparent",
                  border: "none",
                  cursor: "pointer",
                  marginBottom: "-1px",
                }}
              >
                {t.label}
                {active && (
                  <span
                    aria-hidden
                    style={{
                      position: "absolute",
                      left: "0.5rem",
                      right: "0.5rem",
                      bottom: 0,
                      height: "2px",
                      borderRadius: "999px",
                      background: "var(--accent)",
                    }}
                  />
                )}
              </button>
            );
          })}
        </nav>
      </header>

      {tab === "profile" && <ProfileSettingsPanel />}
      {tab === "storage" && <StorageSection />}
      {tab === "appearance" && <AppearanceSection />}
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = value >= 10 || unit === 0 ? 0 : 1;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

function formatImportDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

const ATTACHMENT_PAGE_SIZE = 20;

const sectionTitle: CSSProperties = {
  fontSize: "0.938rem",
  fontWeight: 600,
  color: "var(--text)",
  margin: 0,
};

const sectionHint: CSSProperties = {
  margin: "0.25rem 0 0",
  fontSize: "0.813rem",
  color: "var(--muted)",
};

const tableWrap: CSSProperties = {
  overflowX: "auto",
  border: "1px solid var(--border)",
  borderRadius: "8px",
};

const thStyle: CSSProperties = {
  padding: "0.5rem 0.75rem",
  fontWeight: 500,
  color: "var(--muted)",
  background: "var(--elevated)",
  borderBottom: "1px solid var(--border)",
  textAlign: "left",
  fontSize: "0.813rem",
};

const tdStyle: CSSProperties = {
  padding: "0.5rem 0.75rem",
  borderBottom: "1px solid var(--border)",
  fontSize: "0.813rem",
  color: "var(--text)",
};

interface ImportRow {
  id: number;
  source: string;
  started_at: string;
  finished_at: string | null;
  message_count: number;
  attachment_count: number;
}

interface TopAttachment {
  id: number;
  original_name: string | null;
  mime_type: string | null;
  size_bytes: number;
  conversation_title: string | null;
  chat_identifier: string;
}

function StorageSection() {
  const [imports, setImports] = useState<ImportRow[]>([]);
  const [totalBytes, setTotalBytes] = useState(0);
  const [attachmentCount, setAttachmentCount] = useState(0);
  const [topAttachments, setTopAttachments] = useState<TopAttachment[]>([]);
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  useEffect(() => {
    setLoading(true);
    setError("");
    Promise.all([
      apiClient.get<{ imports: ImportRow[] }>("/v1/imports"),
      apiClient.get<{
        total_bytes: number;
        attachment_count: number;
        top_attachments: TopAttachment[];
      }>("/v1/account/storage"),
    ])
      .then(([importsRes, usageRes]) => {
        setImports(importsRes.imports ?? []);
        setTotalBytes(usageRes.total_bytes ?? 0);
        setAttachmentCount(usageRes.attachment_count ?? 0);
        setTopAttachments(usageRes.top_attachments ?? []);
        setPage(0);
      })
      .catch((e) => {
        setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => setLoading(false));
  }, []);

  if (loading) {
    return <div style={{ fontSize: "0.875rem", color: "var(--muted)" }}>Loading storage…</div>;
  }

  const pageCount = Math.max(1, Math.ceil(topAttachments.length / ATTACHMENT_PAGE_SIZE));
  const pageRows = topAttachments.slice(
    page * ATTACHMENT_PAGE_SIZE,
    page * ATTACHMENT_PAGE_SIZE + ATTACHMENT_PAGE_SIZE,
  );

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "2rem" }}>
      {error && (
        <div
          style={{
            fontSize: "0.813rem",
            color: "var(--danger)",
            background: "var(--danger-soft-bg)",
            border: "1px solid var(--danger-soft-border)",
            borderRadius: "6px",
            padding: "0.5rem 0.75rem",
          }}
        >
          {error}
        </div>
      )}

      <section>
        <h3 style={sectionTitle}>Usage</h3>
        <p style={sectionHint}>Attachment storage for this account (original file sizes).</p>
        <div
          style={{
            marginTop: "0.75rem",
            border: "1px solid var(--border)",
            borderRadius: "8px",
            padding: "0.75rem 1rem",
            background: "var(--elevated)",
          }}
        >
          <div style={{ fontSize: "1.375rem", fontWeight: 600, color: "var(--text)" }}>
            {formatBytes(totalBytes)}
          </div>
          <div style={{ marginTop: "0.25rem", fontSize: "0.813rem", color: "var(--muted)" }}>
            {attachmentCount.toLocaleString()} attachment{attachmentCount === 1 ? "" : "s"}
          </div>
        </div>
      </section>

      <section>
        <h3 style={sectionTitle}>Import history</h3>
        <p style={sectionHint}>Each vault push or CLI import recorded for this account.</p>
        {imports.length === 0 ? (
          <p style={{ ...sectionHint, marginTop: "0.75rem" }}>No imports recorded yet.</p>
        ) : (
          <div style={{ ...tableWrap, marginTop: "0.75rem" }}>
            <table style={{ width: "100%", borderCollapse: "collapse" }}>
              <thead>
                <tr>
                  <th style={thStyle}>Date</th>
                  <th style={thStyle}>Import type</th>
                  <th style={{ ...thStyle, textAlign: "right" }}>Messages</th>
                  <th style={{ ...thStyle, textAlign: "right" }}>Attachments</th>
                </tr>
              </thead>
              <tbody>
                {imports.map((row) => (
                  <tr key={row.id}>
                    <td style={tdStyle}>
                      {formatImportDate(row.finished_at ?? row.started_at)}
                    </td>
                    <td style={tdStyle}>{row.source}</td>
                    <td style={{ ...tdStyle, textAlign: "right", fontVariantNumeric: "tabular-nums" }}>
                      {row.message_count.toLocaleString()}
                    </td>
                    <td style={{ ...tdStyle, textAlign: "right", fontVariantNumeric: "tabular-nums" }}>
                      {row.attachment_count.toLocaleString()}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section>
        <h3 style={sectionTitle}>Largest attachments</h3>
        <p style={sectionHint}>
          Top {topAttachments.length || 100} attachments by file size
          {topAttachments.length > ATTACHMENT_PAGE_SIZE
            ? ` · ${ATTACHMENT_PAGE_SIZE} per page`
            : ""}
          .
        </p>
        {topAttachments.length === 0 ? (
          <p style={{ ...sectionHint, marginTop: "0.75rem" }}>No attachments with sizes yet.</p>
        ) : (
          <div style={{ marginTop: "0.75rem", display: "flex", flexDirection: "column", gap: "0.75rem" }}>
            <div style={tableWrap}>
              <table style={{ width: "100%", borderCollapse: "collapse" }}>
                <thead>
                  <tr>
                    <th style={thStyle}>Name</th>
                    <th style={thStyle}>Conversation</th>
                    <th style={{ ...thStyle, textAlign: "right" }}>Size</th>
                  </tr>
                </thead>
                <tbody>
                  {pageRows.map((row) => (
                    <tr key={row.id}>
                      <td
                        style={{
                          ...tdStyle,
                          maxWidth: "14rem",
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                          whiteSpace: "nowrap",
                        }}
                      >
                        {row.original_name || row.mime_type || `Attachment ${row.id}`}
                      </td>
                      <td style={tdStyle}>
                        {row.conversation_title || row.chat_identifier}
                      </td>
                      <td style={{ ...tdStyle, textAlign: "right", fontVariantNumeric: "tabular-nums" }}>
                        {formatBytes(row.size_bytes)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {topAttachments.length > ATTACHMENT_PAGE_SIZE && (
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  gap: "0.75rem",
                }}
              >
                <span style={{ fontSize: "0.75rem", color: "var(--muted)" }}>
                  Page {page + 1} of {pageCount}
                </span>
                <div style={{ display: "flex", gap: "0.5rem" }}>
                  <Button
                    disabled={page <= 0}
                    onClick={() => setPage((p) => Math.max(0, p - 1))}
                    style={{ padding: "0.375rem 0.75rem", fontSize: "0.813rem" }}
                  >
                    Back
                  </Button>
                  <Button
                    disabled={page >= pageCount - 1}
                    onClick={() => setPage((p) => Math.min(pageCount - 1, p + 1))}
                    style={{ padding: "0.375rem 0.75rem", fontSize: "0.813rem" }}
                  >
                    Next
                  </Button>
                </div>
              </div>
            )}
          </div>
        )}
      </section>
    </div>
  );
}

function AppearanceSection() {
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
