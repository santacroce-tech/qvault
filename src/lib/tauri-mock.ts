/**
 * Mock layer for Tauri APIs when running in a regular browser (for screenshots/preview).
 * This file is aliased in place of @tauri-apps/api/core when TAURI_ENV is not set.
 */

// Mock invoke — returns demo data for each command
export async function invoke(cmd: string, args?: any): Promise<any> {
  switch (cmd) {
    case "get_history":
      return [
        {
          id: "demo-1",
          filename: "Professional_Services_Agreement.pdf",
          file_type: "pdf",
          file_hash: "a1b2c3d4",
          page_count: 12,
          original_path: "/docs/Professional_Services_Agreement.pdf",
          clean_path: null,
          status: "complete",
          pii_count: 23,
          created_at: "2026-03-18T14:30:00Z",
          completed_at: "2026-03-18T14:30:05Z",
        },
        {
          id: "demo-2",
          filename: "Operating_Agreement_LLC.pdf",
          file_type: "pdf",
          file_hash: "e5f6g7h8",
          page_count: 8,
          original_path: "/docs/Operating_Agreement_LLC.pdf",
          clean_path: "/docs/Operating_Agreement_LLC_clean.pdf",
          status: "complete",
          pii_count: 15,
          created_at: "2026-03-17T10:15:00Z",
          completed_at: "2026-03-17T10:15:03Z",
        },
        {
          id: "demo-3",
          filename: "NDA_Confidential.docx",
          file_type: "docx",
          file_hash: "i9j0k1l2",
          page_count: 4,
          original_path: "/docs/NDA_Confidential.docx",
          clean_path: null,
          status: "review",
          pii_count: 9,
          created_at: "2026-03-16T16:45:00Z",
          completed_at: null,
        },
      ];
    case "search_history":
      return [];
    case "get_entity_summary":
      return [
        { normalized_text: "John Smith", category: "PersonName", occurrence_count: 7, document_count: 3 },
        { normalized_text: "jane.doe@acmecorp.com", category: "Email", occurrence_count: 4, document_count: 2 },
        { normalized_text: "Acme Holdings LLC", category: "CompanyName", occurrence_count: 5, document_count: 2 },
        { normalized_text: "***-**-4589", category: "SocialSecurityNumber", occurrence_count: 2, document_count: 1 },
        { normalized_text: "+1 (555) 234-5678", category: "PhoneNumber", occurrence_count: 3, document_count: 2 },
        { normalized_text: "Maria da Silva", category: "PersonName", occurrence_count: 3, document_count: 1 },
        { normalized_text: "CS Digital Ventures Inc", category: "CompanyName", occurrence_count: 2, document_count: 1 },
        { normalized_text: "192.168.1.100", category: "IpAddress", occurrence_count: 1, document_count: 1 },
        { normalized_text: "DE123456789", category: "TaxId", occurrence_count: 1, document_count: 1 },
        { normalized_text: "123.456.789-00", category: "NationalId", occurrence_count: 2, document_count: 1 },
      ];
    case "get_redactions":
      return [
        { id: "r1", document_id: args?.documentId, page: 1, start_offset: 120, end_offset: 131, original_text: "John Smith", replacement: "[REDACTED]", category: "person_name", detection_layer: "heuristic", confidence: 0.92, status: "pending", reviewed_at: null },
        { id: "r2", document_id: args?.documentId, page: 1, start_offset: 200, end_offset: 223, original_text: "jane.doe@acmecorp.com", replacement: "[EMAIL]", category: "email", detection_layer: "regex", confidence: 0.95, status: "approved", reviewed_at: "2026-03-18T14:31:00Z" },
        { id: "r3", document_id: args?.documentId, page: 1, start_offset: 340, end_offset: 351, original_text: "555-234-5678", replacement: "[PHONE]", category: "phone", detection_layer: "regex", confidence: 0.95, status: "pending", reviewed_at: null },
        { id: "r4", document_id: args?.documentId, page: 2, start_offset: 45, end_offset: 56, original_text: "***-**-4589", replacement: "[SSN]", category: "ssn", detection_layer: "regex", confidence: 0.95, status: "approved", reviewed_at: "2026-03-18T14:31:00Z" },
        { id: "r5", document_id: args?.documentId, page: 2, start_offset: 150, end_offset: 170, original_text: "Acme Holdings LLC", replacement: "[COMPANY]", category: "company_name", detection_layer: "heuristic", confidence: 0.90, status: "pending", reviewed_at: null },
        { id: "r6", document_id: args?.documentId, page: 2, start_offset: 210, end_offset: 216, original_text: "Santos", replacement: "[REDACTED NAME]", category: "PersonName", detection_layer: "known-entity", confidence: 0.45, status: "pending", reviewed_at: null },
      ];
    case "get_document":
      return {
        id: args?.id || "demo-1",
        filename: "Professional_Services_Agreement.pdf",
        file_type: "pdf",
        file_hash: "a1b2c3d4",
        page_count: 12,
        original_path: "/docs/Professional_Services_Agreement.pdf",
        clean_path: null,
        status: "review",
        pii_count: 23,
        created_at: "2026-03-18T14:30:00Z",
        completed_at: null,
      };
    case "get_document_entities":
      return [];
    case "get_document_pages":
      return [];
    case "get_document_bytes": {
      // Fetch the example PDF from /public and return as base64
      const resp = await fetch("/example-contract.pdf");
      const buf = await resp.arrayBuffer();
      const bytes = new Uint8Array(buf);
      let binary = "";
      for (let i = 0; i < bytes.length; i++) {
        binary += String.fromCharCode(bytes[i]);
      }
      return btoa(binary);
    }
    case "get_document_text":
      return "This is sample document text for preview purposes.";
    case "upload_document":
      return "demo-new";
    case "scan_document_text":
      return 5;
    case "ai_capabilities":
      return {
        supported: true,
        sidecarBundled: true,
        sidecarRunning: false,
        sidecarAvailable: true,
        sidecarVersion: null,
        unavailableReason: null,
        ocrInstalled: false,
        ocrAvailable: false,
        ocrDpi: 200,
        layer3Enabled: true,
        layer3Model: "qwen2.5-0.5b-instruct-q4_k_m",
        systemMemory: 17179869184,
        layer3Ready: true,
        embeddingInstalled: false,
        entityEmbeddingsEnabled: true,
        embeddingReady: false,
        models: [
          { id: "easyocr-craft-mlt-25k", name: "CRAFT text detector (EasyOCR, GGUF)", purpose: "ocr-detector", size: 83133856, sha256: "74501993caf4581ce09b280f49a1b3d249c0f5a78496e047718b398457a875aa", licence: "MIT / Apache-2.0", source: "QVAC model registry", downloadable: false, manualUrl: "https://github.com/santacroce-tech/qvault/releases/download/ocr-models-v1/craft_mlt_25k.gguf", installed: false, custom: false, defaultLlm: false, ramNeeded: 0, note: null, download: null, fit: null },
          { id: "easyocr-latin-g2", name: "Latin text recognizer (EasyOCR gen-2, GGUF)", purpose: "ocr-recognizer", size: 15396512, sha256: "dd1c7a436e9175904e63683e939635892f45c8c6377d5e14e87cf8b175e10768", licence: "Apache-2.0", source: "QVAC model registry", downloadable: false, manualUrl: "https://github.com/santacroce-tech/qvault/releases/download/ocr-models-v1/latin_g2.gguf", installed: false, custom: false, defaultLlm: false, ramNeeded: 0, note: null, download: null, fit: null },
          { id: "qwen2.5-0.5b-instruct-q4_k_m", name: "Qwen2.5 0.5B Instruct (Q4_K_M)", purpose: "llm", size: 491400032, sha256: "", licence: "Apache-2.0", source: "Hugging Face Qwen/Qwen2.5-0.5B-Instruct-GGUF", downloadable: true, manualUrl: null, installed: true, custom: false, defaultLlm: true, ramNeeded: 0, note: null, download: null, fit: { verdict: "likely-fits", evidence: "native-fit" } },
          { id: "nomic-embed-text-v2-moe-q8_0", name: "Nomic Embed Text v2 MoE (Q8_0)", purpose: "embedding", size: 512225120, sha256: "06e7a7e594a26985523c18383aba4aad39fe6e14f08ffc6ab5b554e1ccdc3cff", licence: "Apache-2.0", source: "Hugging Face nomic-ai/nomic-embed-text-v2-moe-GGUF @ ffbcf4c", downloadable: true, manualUrl: null, installed: false, custom: false, defaultLlm: false, ramNeeded: 1879048192, note: "Optional. Adds similarity-based suggestions when grouping name variants; grouping works without it.", download: null, fit: null },
          { id: "qwen2.5-1.5b-instruct-q4_k_m", name: "Qwen2.5 1.5B Instruct (Q4_K_M)", purpose: "llm", size: 1117320736, sha256: "", licence: "Apache-2.0", source: "Hugging Face Qwen/Qwen2.5-1.5B-Instruct-GGUF", downloadable: true, manualUrl: null, installed: false, custom: false, defaultLlm: false, ramNeeded: 0, note: null, download: { received: 402653184, total: 1117320736, active: true, error: null }, fit: null },
        ],
      };
    case "classify_pdf_pages":
      return (args?.textChars ?? []).map((c: number, page: number) => ({
        page, textChars: c, imageCoverage: 0, needsOcr: c < 32,
        reason: c === 0 ? "noText" : c < 32 ? "littleText" : null,
      }));
    case "ocr_status":
      return { available: false, code: "models_missing", reason: "OCR models are not installed", dpi: 200 };
    case "get_ocr_pages":
      return [];
    case "regroup_entities":
      return { variants: 9, groups: 6, mergedGroups: 2, proposals: 2, embedded: 0, embeddingModel: null, embeddingError: null, embedMs: 0, durationMs: 3 };
    case "get_entity_groups":
      return [
        { id: "g1", category: "PersonName", label: "Maria Fernanda Santos", occurrences: 9, documents: 3, approved: 6, proposals: 2, variants: [
          { id: "v1", text: "Maria Santos", occurrences: 5, documents: 3, approved: 4, pinned: false, hasEmbedding: false },
          { id: "v2", text: "SANTOS, Maria", occurrences: 2, documents: 1, approved: 1, pinned: false, hasEmbedding: false },
          { id: "v3", text: "Maria Fernanda Santos", occurrences: 2, documents: 1, approved: 1, pinned: true, hasEmbedding: false },
        ] },
        { id: "g2", category: "PersonName", label: "M. Santos", occurrences: 2, documents: 1, approved: 0, proposals: 1, variants: [
          { id: "v4", text: "M. Santos", occurrences: 2, documents: 1, approved: 0, pinned: false, hasEmbedding: false },
        ] },
        { id: "g3", category: "PersonName", label: "João Santos", occurrences: 3, documents: 2, approved: 3, proposals: 0, variants: [
          { id: "v5", text: "João Santos", occurrences: 3, documents: 2, approved: 3, pinned: false, hasEmbedding: false },
        ] },
        { id: "g4", category: "CompanyName", label: "Acme Holdings LLC", occurrences: 6, documents: 2, approved: 4, proposals: 1, variants: [
          { id: "v6", text: "Acme Holdings LLC", occurrences: 4, documents: 2, approved: 4, pinned: false, hasEmbedding: false },
          { id: "v7", text: "ACME HOLDINGS, L.L.C.", occurrences: 2, documents: 1, approved: 0, pinned: false, hasEmbedding: false },
        ] },
        { id: "g5", category: "CompanyName", label: "Acme", occurrences: 1, documents: 1, approved: 0, proposals: 1, variants: [
          { id: "v8", text: "Acme", occurrences: 1, documents: 1, approved: 0, pinned: false, hasEmbedding: false },
        ] },
        { id: "g6", category: "Email", label: "jane.doe@acmecorp.com", occurrences: 4, documents: 2, approved: 2, proposals: 0, variants: [
          { id: "v9", text: "jane.doe@acmecorp.com", occurrences: 4, documents: 2, approved: 2, pinned: false, hasEmbedding: false },
        ] },
      ];
    case "get_entity_proposals":
      return [
        { groupA: "g1", groupB: "g2", labelA: "Maria Fernanda Santos", labelB: "M. Santos", variantA: "Maria Santos", variantB: "M. Santos", category: "PersonName", rule: "initial", score: 0.7, cosine: null, ambiguous: false },
        { groupA: "g4", groupB: "g5", labelA: "Acme Holdings LLC", labelB: "Acme", variantA: "Acme Holdings LLC", variantB: "Acme", category: "CompanyName", rule: "partial-name", score: 0.6, cosine: null, ambiguous: false },
      ];
    case "get_entity_group_documents":
      return [
        { documentId: "demo-1", filename: "Professional_Services_Agreement.pdf", createdAt: "2026-03-18T14:30:00Z", occurrences: 4 },
        { documentId: "demo-2", filename: "Operating_Agreement_LLC.pdf", createdAt: "2026-03-17T10:15:00Z", occurrences: 1 },
      ];
    case "get_redaction_notes":
      return [
        { redactionId: "r6", kind: "known-entity", rule: "honorific-surname", sourceText: "Maria Santos", sourceApproved: true, documents: 3, groupId: "g1", groupLabel: "Maria Fernanda Santos" },
      ];
    default:
      console.log(`[tauri-mock] Unhandled invoke: ${cmd}`, args);
      return null;
  }
}

export function transformCallback(_callback: any, _once?: boolean): number {
  return 0;
}
