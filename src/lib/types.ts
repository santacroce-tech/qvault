export type PiiCategory =
  | "PersonName"
  | "Email"
  | "PhoneNumber"
  | "Address"
  | "DateOfBirth"
  | "Date"
  | "MonetaryAmount"
  | "SocialSecurityNumber"
  | "NationalId"
  | "PassportNumber"
  | "DriversLicense"
  | "BankAccount"
  | "CreditCard"
  | "IBAN"
  | "TaxId"
  | "CaseNumber"
  | "CourtFileNumber"
  | "ClientId"
  | "IpAddress"
  | "Url"
  | "LicensePlate"
  | "MedicalRecordNumber"
  | "CompanyName";

export type DocumentStatus = "processing" | "review" | "complete";
export type RedactionStatus = "pending" | "approved" | "rejected";

export interface Document {
  id: string;
  filename: string;
  file_type: "pdf" | "docx";
  file_hash: string;
  page_count: number;
  original_path: string;
  clean_path: string | null;
  status: DocumentStatus;
  pii_count: number;
  created_at: string;
  completed_at: string | null;
}

export interface Redaction {
  id: string;
  document_id: string;
  page: number;
  start_offset: number;
  end_offset: number;
  original_text: string;
  replacement: string;
  category: PiiCategory;
  detection_layer: "regex" | "ner" | "heuristic" | "llm" | "manual" | "known-entity";
  confidence: number;
  status: RedactionStatus;
  reviewed_at: string | null;
}

export interface EntityRow {
  id: string;
  document_id: string;
  original_text: string;
  normalized_text: string;
  category: string;
  page: number;
  start_offset: number;
  end_offset: number;
  confidence: number;
  detection_layer: string;
  created_at: string;
}

export interface EntitySummary {
  normalized_text: string;
  category: string;
  occurrence_count: number;
  document_count: number;
}

/** One way an entity is written (a distinct string of one category). */
export interface EntityVariant {
  id: string;
  text: string;
  occurrences: number;
  documents: number;
  approved: number;
  /** Placed by a user decision (merge / split); kept on regrouping. */
  pinned: boolean;
  hasEmbedding: boolean;
}

/** Variants grouped as one entity. */
export interface EntityGroup {
  id: string;
  category: string;
  label: string;
  occurrences: number;
  documents: number;
  approved: number;
  variants: EntityVariant[];
  proposals: number;
}

export type MergeRule =
  | "same-name" | "reordered" | "name-extension" | "legal-form" | "same-number" | "initial"
  | "surname-only" | "partial-name" | "similar-spelling" | "different-legal-form" | "acronym"
  | "similar-address" | "embedding";

/** A suggested merge of two groups, waiting for review. */
export interface EntityProposal {
  groupA: string;
  groupB: string;
  labelA: string;
  labelB: string;
  variantA: string;
  variantB: string;
  category: string;
  rule: MergeRule;
  score: number;
  cosine: number | null;
  /** The short form also matches another, different person. */
  ambiguous: boolean;
}

export interface EntityGroupDocument {
  documentId: string;
  filename: string;
  createdAt: string;
  occurrences: number;
}

export interface RegroupReport {
  variants: number;
  groups: number;
  mergedGroups: number;
  proposals: number;
  embedded: number;
  embeddingModel: string | null;
  embeddingError: string | null;
  embedMs: number;
  durationMs: number;
}

export type KnownEntityRule = "same-text" | "reordered" | "honorific-surname" | "initial-surname" | "company-without-legal-form";

/** Why a known-entity suggestion was made. */
export interface RedactionNote {
  redactionId: string;
  kind: "known-entity";
  rule: KnownEntityRule;
  sourceText: string;
  sourceApproved: boolean;
  documents: number;
  groupId: string | null;
  groupLabel: string | null;
}

export interface DocumentPage {
  document_id: string;
  page_number: number;
  raw_text: string;
  cleaned_text: string | null;
  created_at: string;
}

export const PII_COLORS: Record<string, string> = {
  PersonName: "#3B82F6",
  Email: "#3B82F6",
  PhoneNumber: "#3B82F6",
  Address: "#10B981",
  DateOfBirth: "#F59E0B",
  Date: "#F59E0B",
  MonetaryAmount: "#D97706",
  SocialSecurityNumber: "#EF4444",
  NationalId: "#8B5CF6",
  PassportNumber: "#8B5CF6",
  DriversLicense: "#8B5CF6",
  BankAccount: "#EF4444",
  CreditCard: "#EF4444",
  IBAN: "#EF4444",
  TaxId: "#EF4444",
  CaseNumber: "#8B5CF6",
  CourtFileNumber: "#8B5CF6",
  ClientId: "#8B5CF6",
  IpAddress: "#10B981",
  Url: "#10B981",
  LicensePlate: "#8B5CF6",
  MedicalRecordNumber: "#EF4444",
  CompanyName: "#0891B2",
};

export function piiCategoryLabel(cat: PiiCategory, t?: (key: string) => string): string {
  if (t) {
    return t(`pii.${cat}`);
  }
  // Fallback for contexts without i18n
  const labels: Record<PiiCategory, string> = {
    PersonName: "Name",
    Email: "Email",
    PhoneNumber: "Phone",
    Address: "Address",
    DateOfBirth: "Date of Birth",
    Date: "Date",
    MonetaryAmount: "Money",
    SocialSecurityNumber: "SSN",
    NationalId: "National ID",
    PassportNumber: "Passport",
    DriversLicense: "Driver's License",
    BankAccount: "Bank Account",
    CreditCard: "Credit Card",
    IBAN: "IBAN",
    TaxId: "Tax ID",
    CaseNumber: "Case Number",
    CourtFileNumber: "Court File",
    ClientId: "Client ID",
    IpAddress: "IP Address",
    Url: "URL",
    LicensePlate: "License Plate",
    MedicalRecordNumber: "Medical Record",
    CompanyName: "Company",
  };
  return labels[cat];
}
