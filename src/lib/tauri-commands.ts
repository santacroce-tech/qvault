import { invoke } from "@tauri-apps/api/core";
import type {
  Document,
  Redaction,
  EntityRow,
  EntitySummary,
  DocumentPage,
} from "./types";

// Documents
export async function uploadDocument(filePath: string, pageCount: number): Promise<string> {
  return invoke("upload_document", { filePath, pageCount });
}

/** Upload a document from raw bytes (base64-encoded). Used as mobile fallback when native file dialog is unavailable. */
export async function uploadDocumentBytes(filename: string, data: string): Promise<string> {
  return invoke("upload_document_bytes", { filename, data });
}

export async function scanDocumentText(id: string, pages: string[]): Promise<number> {
  return invoke("scan_document_text", { id, pages });
}

export async function addManualRedaction(
  documentId: string,
  page: number,
  startOffset: number,
  endOffset: number,
  originalText: string,
  replacement: string,
  category: string
): Promise<string> {
  return invoke("add_manual_redaction", {
    documentId,
    page,
    startOffset,
    endOffset,
    originalText,
    replacement,
    category,
  });
}

export async function getDocument(id: string): Promise<Document> {
  return invoke("get_document", { id });
}

export async function getDocumentText(id: string, page: number): Promise<string> {
  return invoke("get_document_text", { id, page });
}

export async function extractDocxText(id: string): Promise<string[]> {
  return invoke("extract_docx_text", { id });
}

export async function getDocumentBytes(id: string): Promise<string> {
  return invoke("get_document_bytes", { id });
}

export async function getDocumentEntities(documentId: string): Promise<EntityRow[]> {
  return invoke("get_document_entities", { documentId });
}

export async function getEntitySummary(): Promise<EntitySummary[]> {
  return invoke("get_entity_summary");
}

export async function getDocumentPages(documentId: string): Promise<DocumentPage[]> {
  return invoke("get_document_pages", { documentId });
}

export type RedactionModeType = "black_bar" | "text_replace";

/** Export a redacted DOCX. The backend reads the approved redactions itself. */
export async function exportRedactedDocx(
  id: string,
  redactionMode?: RedactionModeType,
): Promise<string> {
  return invoke("export_redacted_docx", { id, redactionMode });
}

/** A box to burn into a page image, in that image's pixels (origin top-left). */
export interface PixelRect {
  /** Approved redaction whose text this box covers. */
  redactionId: string;
  /** Which occurrence (0-based) of that text on the page; several boxes may share one. */
  occurrence: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

/** A clean render of one page that carries approved redactions. */
export interface RedactedPageInput {
  /** 0-based page index */
  page: number;
  /** PNG, base64. Must be the plain page render: the backend burns the boxes. */
  imageBase64: string;
  rects: PixelRect[];
}

/**
 * Export a redacted PDF. Only pages in `pages` are rasterized; the backend
 * refuses (rejects with a message) if a redaction is missing a box or approved
 * text would remain anywhere in the output.
 */
export async function exportRedactedPdf(
  id: string,
  pages: RedactedPageInput[],
): Promise<string> {
  return invoke("export_redacted_pdf", { id, pages });
}

export async function openFile(path: string): Promise<void> {
  return invoke("open_file", { path });
}

export async function deleteDocument(id: string): Promise<void> {
  return invoke("delete_document", { id });
}

// Redactions
export async function getRedactions(documentId: string): Promise<Redaction[]> {
  return invoke("get_redactions", { documentId });
}

export async function updateRedaction(
  id: string,
  status: string,
  replacement?: string
): Promise<void> {
  return invoke("update_redaction", { id, status, replacement });
}

export async function bulkApprove(
  documentId: string,
  category?: string
): Promise<void> {
  return invoke("bulk_approve", { documentId, category });
}

// LLM
export interface LlmStatus {
  available: boolean;
  enabled: boolean;
  downloading: boolean;
  downloadProgress: number;
  loaded: boolean;
  device: string;
}

export async function getLlmStatus(): Promise<LlmStatus> {
  return invoke("llm_status");
}

export async function downloadLlmModel(): Promise<void> {
  return invoke("llm_download_model");
}

export async function loadLlmModel(): Promise<void> {
  return invoke("llm_load_model");
}

export async function setLlmEnabled(enabled: boolean): Promise<void> {
  return invoke("llm_set_enabled", { enabled });
}

// History
export async function getHistory(
  offset: number,
  limit: number
): Promise<Document[]> {
  return invoke("get_history", { offset, limit });
}

export async function searchHistory(query: string): Promise<Document[]> {
  return invoke("search_history", { query });
}
