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
          <div className="mb-4 flex justify-between">
            <h2 className="m-0 text-[1.125rem]">Sources</h2>
            <button onClick={onClose} className="cursor-pointer border-none bg-none text-[1.25rem] text-muted">×</button>
          </div>

          {sources.length === 0 ? (
            <div className="text-[0.875rem] text-muted">No source data available.</div>
          ) : (
            <>
              {sources.map((s, i) => (
                <div key={i} className="mb-3 rounded bg-elevated p-2">
                  <div className="text-[0.875rem] font-medium">{s.backup_name}</div>
                  <div className="text-[0.75rem] text-muted">
                    {s.message_count.toLocaleString()} messages ({s.percentage}% of total)
                  </div>
                  <div className="text-[0.75rem] text-muted">
                    {s.unique_count.toLocaleString()} unique
                  </div>
                </div>
              ))}
              <div className="mt-2 text-[0.813rem] text-muted">
                Net total: {total.toLocaleString()} unique messages
              </div>
            </>
          )}
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
