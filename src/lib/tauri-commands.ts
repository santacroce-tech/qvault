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

// AI engine (QVAC sidecar) + Model Manager
export type ModelPurpose = "ocr-detector" | "ocr-recognizer" | "llm";
export type FitVerdict = "likely-fits" | "likely-too-large" | "unknown";

export interface FitInfo {
  verdict: FitVerdict;
  evidence: string | null;
}

export interface DownloadState {
  received: number;
  total: number;
  active: boolean;
  error: string | null;
}

export interface ModelInfo {
  id: string;
  name: string;
  purpose: ModelPurpose;
  size: number;
  sha256: string;
  licence: string;
  source: string;
  downloadable: boolean;
  installed: boolean;
  custom: boolean;
  defaultLlm: boolean;
  download: DownloadState | null;
  fit: FitInfo | null;
}

export interface AiCapabilities {
  supported: boolean;
  sidecarBundled: boolean;
  sidecarRunning: boolean;
  sidecarAvailable: boolean;
  sidecarVersion: string | null;
  unavailableReason: string | null;
  ocrInstalled: boolean;
  layer3Enabled: boolean;
  layer3Model: string | null;
  layer3Ready: boolean;
  models: ModelInfo[];
}

export type ImportOutcome =
  | { status: "installed"; id: string }
  | { status: "needsConsent"; sha256: string; size: number; fileName: string }
  | { status: "installedCustom"; id: string };

export interface SelfTestReport {
  sidecar: string | null;
  error: string | null;
  ocrMs: number | null;
  ocrText: string | null;
  ocrError: string | null;
  layer3Model: string | null;
  layer3Ms: number | null;
  layer3Entities: string[];
  layer3Error: string | null;
}

/** `probe` starts the sidecar and computes fit verdicts for installed LLMs. */
export async function getAiCapabilities(probe = false): Promise<AiCapabilities> {
  return invoke("ai_capabilities", { probe });
}

export async function listModels(): Promise<ModelInfo[]> {
  return invoke("ai_list_models");
}

export async function downloadModel(id: string): Promise<void> {
  return invoke("ai_download_model", { id });
}

export async function cancelModelDownload(id: string): Promise<boolean> {
  return invoke("ai_cancel_download", { id });
}

export async function importModel(path: string, acceptUnknown = false): Promise<ImportOutcome> {
  return invoke("ai_import_model", { path, acceptUnknown });
}

export async function deleteModel(id: string): Promise<void> {
  return invoke("ai_delete_model", { id });
}

export async function assessModelFit(id: string): Promise<FitInfo> {
  return invoke("ai_assess_fit", { id });
}

export async function setLayer3(enabled: boolean, modelId?: string): Promise<void> {
  return invoke("ai_set_layer3", { enabled, modelId: modelId ?? null });
}

export async function runAiSelfTest(): Promise<SelfTestReport> {
  return invoke("ai_self_test");
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
