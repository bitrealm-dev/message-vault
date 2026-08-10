import { useState, useEffect } from "react";
import { ModalOverlay, Modal, Dialog } from "react-aria-components";
import { apiClient } from "../lib/api";

interface SourceInfo {
  backup_name: string;
  message_count: number;
  unique_count: number;
  percentage: number;
}

export default function SourcesPanel({
  conversationId,
  onClose,
}: {
  conversationId: string | null;
  onClose: () => void;
}) {
  const [sources, setSources] = useState<SourceInfo[]>([]);

  useEffect(() => {
    if (!conversationId) return;
    apiClient
      .get<{ sources: SourceInfo[] }>(`/v1/export/conversations/${conversationId}/sources`)
      .then((res) => setSources(res.sources))
      .catch(() => setSources([]));
  }, [conversationId]);

  if (!conversationId) return null;

  const total = sources.reduce((sum, s) => sum + s.unique_count, 0);

  return (
    <ModalOverlay
      isOpen={!!conversationId}
      isDismissable
      onOpenChange={(o) => {
        if (o) return;
        onClose();
      }}
      className="fixed inset-0 z-40 bg-[rgba(0,0,0,0.2)]"
    >
      <Modal
        className="fixed top-0 bottom-0 right-0 z-50 w-[320px] overflow-auto bg-panel p-6 shadow-[-2px_0_8px_rgba(0,0,0,0.1)] outline-none"
      >
        <Dialog aria-label="Sources" className="outline-none">
          <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "1rem" }}>
            <h2 style={{ margin: 0, fontSize: "1.125rem" }}>Sources</h2>
            <button onClick={onClose} style={{ border: "none", background: "none", fontSize: "1.25rem", cursor: "pointer", color: "var(--muted)" }}>×</button>
          </div>

          {sources.length === 0 ? (
            <div style={{ fontSize: "0.875rem", color: "var(--muted)" }}>No source data available.</div>
          ) : (
            <>
              {sources.map((s, i) => (
                <div key={i} style={{ marginBottom: "0.75rem", padding: "0.5rem", background: "var(--elevated)", borderRadius: "4px" }}>
                  <div style={{ fontSize: "0.875rem", fontWeight: 500 }}>{s.backup_name}</div>
                  <div style={{ fontSize: "0.75rem", color: "var(--muted)" }}>
                    {s.message_count.toLocaleString()} messages ({s.percentage}% of total)
                  </div>
                  <div style={{ fontSize: "0.75rem", color: "var(--muted)" }}>
                    {s.unique_count.toLocaleString()} unique
                  </div>
                </div>
              ))}
              <div style={{ marginTop: "0.5rem", fontSize: "0.813rem", color: "var(--muted)" }}>
                Net total: {total.toLocaleString()} unique messages
              </div>
            </>
          )}
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
