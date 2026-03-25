import { invoke } from "@tauri-apps/api/core";
import type {
  Document,
  Redaction,
  LicenseInfo,
  EntityRow,
  EntitySummary,
  DocumentPage,
} from "./types";

// Documents
export async function uploadDocument(filePath: string, pageCount: number): Promise<string> {
  return invoke("upload_document", { filePath, pageCount });
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

export interface RedactionRect {
  page: number;
  x: number;
  y: number;
  width: number;
  height: number;
  replacement?: string;
  category?: string;
}

export type RedactionModeType = "black_bar" | "text_replace";

export async function exportClean(
  id: string,
  redactionRects: RedactionRect[],
  redactionMode?: RedactionModeType,
): Promise<string> {
  return invoke("export_clean", { id, redactionRects, redactionMode });
}

export interface PageImageData {
  jpegBase64: string;
  width: number;
  height: number;
  pageWidth: number;
  pageHeight: number;
}

export async function exportCleanFlat(
  id: string,
  pages: PageImageData[],
): Promise<string> {
  return invoke("export_clean_flat", { id, pages });
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

// License
export async function getLicense(): Promise<LicenseInfo | null> {
  return invoke("get_license");
}

export async function activateLicense(licenseKey: string): Promise<LicenseInfo> {
  return invoke("activate_license", { licenseKey });
}

export async function deactivateLicense(): Promise<void> {
  return invoke("deactivate_license");
}

export async function checkLicense(): Promise<boolean> {
  return invoke("check_license");
}

export async function validateLicenseOnline(): Promise<boolean> {
  return invoke("validate_license_online");
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
