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
    default:
      console.log(`[tauri-mock] Unhandled invoke: ${cmd}`, args);
      return null;
  }
}

export function transformCallback(_callback: any, _once?: boolean): number {
  return 0;
}
