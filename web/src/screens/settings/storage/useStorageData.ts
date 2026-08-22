import { useEffect, useState } from "react";
import { apiClient } from "../../../lib/api";
import type { ImportDetailResponse, ImportRow, TopAttachment } from "./storageUtils";

export function useStorageData() {
  const [imports, setImports] = useState<ImportRow[]>([]);
  const [totalBytes, setTotalBytes] = useState(0);
  const [attachmentCount, setAttachmentCount] = useState(0);
  const [topAttachments, setTopAttachments] = useState<TopAttachment[]>([]);
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [selectedImportId, setSelectedImportId] = useState<number | null>(null);
  const [selectedImport, setSelectedImport] = useState<ImportDetailResponse | null>(null);
  const [selectedImportLoading, setSelectedImportLoading] = useState(false);
  const [selectedImportError, setSelectedImportError] = useState("");

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

  useEffect(() => {
    if (selectedImportId == null) return;

    const controller = new AbortController();
    setSelectedImportLoading(true);
    setSelectedImportError("");

    apiClient
      .get<ImportDetailResponse>(`/v1/imports/${selectedImportId}`, {
        signal: controller.signal,
      })
      .then((detail) => {
        setSelectedImport(detail);
      })
      .catch((err) => {
        if (controller.signal.aborted) return;
        setSelectedImport(null);
        setSelectedImportError(
          err instanceof Error ? err.message : "Couldn’t load import details.",
        );
      })
      .finally(() => {
        if (!controller.signal.aborted) {
          setSelectedImportLoading(false);
        }
      });

    return () => controller.abort();
  }, [selectedImportId]);

  const closeImportDetail = () => {
    setSelectedImportId(null);
    setSelectedImport(null);
    setSelectedImportLoading(false);
    setSelectedImportError("");
  };

  const toggleImportDetail = (importId: number) => {
    if (selectedImportId === importId) {
      closeImportDetail();
      return;
    }
    setSelectedImportId(importId);
    setSelectedImport(null);
    setSelectedImportError("");
  };

  return {
    imports,
    totalBytes,
    attachmentCount,
    topAttachments,
    page,
    setPage,
    loading,
    error,
    selectedImportId,
    selectedImport,
    selectedImportLoading,
    selectedImportError,
    closeImportDetail,
    toggleImportDetail,
  };
}
