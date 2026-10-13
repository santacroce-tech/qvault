import { invoke } from "@tauri-apps/api/core";
import type {
  Document,
  Redaction,
  EntityRow,
  EntitySummary,
  DocumentPage,
  EntityGroup,
  EntityProposal,
  EntityGroupDocument,
  RegroupReport,
  RedactionNote,
  DocumentLearning,
  NotPersonalDataItem,
  ListFileItem,
  ListImportPreview,
  ListCandidate,
  LearningSummary,
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

// Entity groups (Phase 4)

/** Bring groups up to date. `embed` also embeds new variants with the embedding model (if installed). */
export async function regroupEntities(embed = false): Promise<RegroupReport> {
  return invoke("regroup_entities", { embed });
}

export async function getEntityGroups(): Promise<EntityGroup[]> {
  return invoke("get_entity_groups");
}

export async function getEntityProposals(): Promise<EntityProposal[]> {
  return invoke("get_entity_proposals");
}

export async function getEntityGroupDocuments(groupId: string): Promise<EntityGroupDocument[]> {
  return invoke("get_entity_group_documents", { groupId });
}

/** Merge `others` into `target` (same category only). */
export async function mergeEntityGroups(target: string, others: string[]): Promise<void> {
  return invoke("merge_entity_groups", { target, others });
}

/** Take a variant out of its group; returns the new group id. */
export async function splitEntityVariant(variantId: string): Promise<string> {
  return invoke("split_entity_variant", { variantId });
}

/** "These are different": keep two groups apart. */
export async function dismissEntityProposal(groupA: string, groupB: string): Promise<void> {
  return invoke("dismiss_entity_proposal", { groupA, groupB });
}

/** Explanations of known-entity suggestions in a document. */
export async function getRedactionNotes(documentId: string): Promise<RedactionNote[]> {
  return invoke("get_redaction_notes", { documentId });
}

// Learning from review decisions (§13)

/** Findings rejected before and findings the "Not personal data" list set aside in a document. */
export async function getDocumentLearning(documentId: string): Promise<DocumentLearning> {
  return invoke("get_document_learning", { documentId });
}

/** Reject every pending finding of the document that was rejected before (a user action). */
export async function rejectPreviouslyRejected(documentId: string): Promise<number> {
  return invoke("reject_previously_rejected", { documentId });
}

/** Put a suppressed finding back into the review list (pending). */
export async function restoreSuppressedFinding(id: string, documentId: string): Promise<string> {
  return invoke("restore_suppressed_finding", { id, documentId });
}

export async function getNotPersonalData(): Promise<NotPersonalDataItem[]> {
  return invoke("get_not_personal_data");
}

export async function addNotPersonalData(category: string, text: string, source: "review" | "settings" | "suggestion", note?: string): Promise<string> {
  return invoke("add_not_personal_data", { category, text, note: note ?? null, source });
}

export async function removeNotPersonalData(id: string): Promise<boolean> {
  return invoke("remove_not_personal_data", { id });
}

export async function exportNotPersonalData(path: string): Promise<number> {
  return invoke("export_not_personal_data", { path });
}

export async function previewNotPersonalDataImport(path: string): Promise<ListImportPreview> {
  return invoke("preview_not_personal_data_import", { path });
}

export async function applyNotPersonalDataImport(items: ListFileItem[]): Promise<number> {
  return invoke("apply_not_personal_data_import", { items });
}

export async function getNotPersonalDataCandidates(): Promise<ListCandidate[]> {
  return invoke("get_not_personal_data_candidates");
}

export async function getLearningSummary(): Promise<LearningSummary> {
  return invoke("get_learning_summary");
}

export async function clearReviewMemory(): Promise<number> {
  return invoke("clear_review_memory");
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
 * text would remain anywhere in the output, including text still readable by
 * OCR on scanned pages. If a page that needs OCR was never scanned, it rejects
 * with `OCR_UNSCANNED:<0-based pages>|<message>` unless `acknowledgeUnscanned`.
 */
export async function exportRedactedPdf(
  id: string,
  pages: RedactedPageInput[],
  acknowledgeUnscanned = false,
): Promise<string> {
  return invoke("export_redacted_pdf", { id, pages, acknowledgeUnscanned });
}

// OCR (scanned pages and imported images)
export type OcrReason = "noText" | "littleText" | "mostlyImage";

export interface PageClass {
  /** 0-based */
  page: number;
  textChars: number;
  imageCoverage: number;
  needsOcr: boolean;
  reason: OcrReason | null;
}

export interface OcrStatus {
  available: boolean;
  code: "ok" | "unsupported" | "not_bundled" | "models_missing" | "engine_unavailable";
  reason: string | null;
  dpi: number;
}

/** A box on the page, in PDF points from the top-left of the displayed page. */
export interface OcrWordBox {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  line: number;
}

/** A run of the OCR page text mapped to one box (or none, for separators). */
export interface OcrSegment {
  text: string;
  boxIndex: number | null;
}

export interface OcrPageLayout {
  widthPt: number;
  heightPt: number;
  dpi: number;
  blocks: number;
  /** Normalised page text (= concatenation of segment texts). */
  text: string;
  boxes: OcrWordBox[];
  segments: OcrSegment[];
}

export type OcrPageStatus = "done" | "unavailable" | "cancelled" | "failed";

export interface OcrPageInfo {
  page: number;
  status: OcrPageStatus;
  reason: string | null;
  dpi: number | null;
  layout: OcrPageLayout | null;
}

export interface UnscannedPage {
  page: number;
  status: Exclude<OcrPageStatus, "done">;
  reason: string | null;
}

export async function classifyPdfPages(id: string, textChars: number[]): Promise<PageClass[]> {
  return invoke("classify_pdf_pages", { id, textChars });
}

export async function getOcrStatus(): Promise<OcrStatus> {
  return invoke("ocr_status");
}

/** OCR one page rendered at `dpi` (PNG base64). Errors start with OCR_CANCELLED / OCR_UNAVAILABLE / OCR_FAILED. */
export async function ocrPage(id: string, page: number, imageBase64: string, dpi: number): Promise<OcrPageLayout> {
  return invoke("ocr_page", { id, page, imageBase64, dpi });
}

export async function cancelOcr(id: string): Promise<boolean> {
  return invoke("ocr_cancel", { id });
}

export async function finishOcr(id: string, unscanned: UnscannedPage[]): Promise<void> {
  return invoke("ocr_finish", { id, unscanned });
}

export async function getOcrPages(id: string): Promise<OcrPageInfo[]> {
  return invoke("get_ocr_pages", { id });
}

export async function setOcrDpi(dpi: number): Promise<void> {
  return invoke("ai_set_ocr_dpi", { dpi });
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
export type ModelPurpose = "ocr-detector" | "ocr-recognizer" | "llm" | "embedding" | "pii-classifier";
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
  manualUrl: string | null;
  installed: boolean;
  custom: boolean;
  defaultLlm: boolean;
  download: DownloadState | null;
  fit: FitInfo | null;
  /** Approximate memory the model needs to run (bytes; 0 = unknown). */
  ramNeeded: number;
  /** Catalog note shown before download (licence or role caveats). */
  note: string | null;
  /** Number of files (multi-file models; `size` is their total). */
  fileCount: number;
}

export type Layer3Detector = "privacy-filter" | "llm";

export interface AiCapabilities {
  supported: boolean;
  sidecarBundled: boolean;
  sidecarRunning: boolean;
  sidecarAvailable: boolean;
  sidecarVersion: string | null;
  unavailableReason: string | null;
  ocrInstalled: boolean;
  /** OCR can run now (engine bundled and usable, models installed). */
  ocrAvailable: boolean;
  ocrDpi: number;
  layer3Enabled: boolean;
  layer3Model: string | null;
  layer3Ready: boolean;
  /** The entity-grouping embedding model is installed. */
  embeddingInstalled: boolean;
  /** User setting (default on). */
  entityEmbeddingsEnabled: boolean;
  /** Embeddings will be used for entity grouping. */
  embeddingReady: boolean;
  models: ModelInfo[];
  /** Physical memory of this computer, when it can be read. */
  systemMemory: number | null;
  /** Effective Layer 3 detector. */
  layer3Detector: Layer3Detector;
  /** Also run the language model after Privacy Filter. */
  layer3Thorough: boolean;
  /** Privacy Filter's files are installed. */
  pfInstalled: boolean;
  /** The engine can run Privacy Filter here (null = not checked yet). */
  pfEngine: boolean | null;
  pfEngineReason: string | null;
}

export type ImportOutcome =
  | { status: "installed"; id: string }
  | { status: "needsConsent"; sha256: string; size: number; fileName: string }
  | { status: "installedCustom"; id: string }
  | { status: "incomplete"; id: string; missing: string[] };

export interface ImportSummary {
  outcomes: ImportOutcome[];
  /** Selected files that belong to no known model. */
  ignored: number;
}

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
  embeddingMs: number | null;
  embeddingDims: number | null;
  embeddingError: string | null;
  pfMs: number | null;
  pfEntities: string[];
  pfError: string | null;
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

/** Import several files and/or folders (models made of several files). */
export async function importModelPaths(paths: string[]): Promise<ImportSummary> {
  return invoke("ai_import_model_paths", { paths });
}

/** Open an import-only model's pinned download link in the system browser. */
export async function openManualDownload(id: string): Promise<void> {
  return invoke("ai_open_manual_download", { id });
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

export async function setLayer3Detector(detector?: Layer3Detector, thorough?: boolean): Promise<void> {
  return invoke("ai_set_layer3_detector", { detector: detector ?? null, thorough: thorough ?? null });
}

export async function setEntityEmbeddings(enabled: boolean): Promise<void> {
  return invoke("ai_set_entity_embeddings", { enabled });
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
