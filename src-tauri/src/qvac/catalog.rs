//! Pinned model catalog. Every entry carries the exact byte size and SHA-256
//! of the file Qvault accepts; downloads and imports are verified against
//! both. URLs are HTTPS and pinned to an immutable revision. Models without a
//! first-party HTTPS host have `url: None` and can only be imported from a
//! file (never fetched over P2P / the QVAC registry).

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelPurpose {
    OcrDetector,
    OcrRecognizer,
    Llm,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModel {
    /// Stable id used by the UI, settings and audit log.
    pub id: &'static str,
    pub name: &'static str,
    pub purpose: ModelPurpose,
    /// File name under `<app data>/models/`.
    pub file_name: &'static str,
    /// Pinned HTTPS URL, or `None` when the model is import-only.
    pub url: Option<&'static str>,
    pub size: u64,
    pub sha256: &'static str,
    pub licence: &'static str,
    pub licence_url: &'static str,
    /// Where the bytes come from, for the UI and ATTRIBUTION.txt.
    pub source: &'static str,
    /// Default Layer 3 model.
    pub default_llm: bool,
}

pub const CATALOG: &[CatalogModel] = &[
    CatalogModel {
        id: "easyocr-craft-mlt-25k",
        name: "CRAFT text detector (EasyOCR, GGUF)",
        purpose: ModelPurpose::OcrDetector,
        file_name: "craft_mlt_25k.gguf",
        // No official HTTPS host exists for QVAC's GGUF conversion (the QVAC
        // registry serves it over Hyperswarm only). Import-only until Qvault
        // mirrors it on Hugging Face; see docs/qvac-integration.md §4.3.
        url: None,
        size: 83_133_856,
        sha256: "74501993caf4581ce09b280f49a1b3d249c0f5a78496e047718b398457a875aa",
        licence: "MIT (CRAFT weights, NAVER Clova AI); GGUF conversion Apache-2.0 (Tether QVAC)",
        licence_url: "https://github.com/clovaai/CRAFT-pytorch/blob/master/LICENSE",
        source: "QVAC model registry OCR_CRAFT (qvac_models_compiled/ocr/gguf/easyocr/2026-05-14)",
        default_llm: false,
    },
    CatalogModel {
        id: "easyocr-latin-g2",
        name: "Latin text recognizer (EasyOCR gen-2, GGUF)",
        purpose: ModelPurpose::OcrRecognizer,
        file_name: "latin_g2.gguf",
        url: None,
        size: 15_396_512,
        sha256: "dd1c7a436e9175904e63683e939635892f45c8c6377d5e14e87cf8b175e10768",
        licence: "Apache-2.0 (EasyOCR, Jaided AI); GGUF conversion Apache-2.0 (Tether QVAC)",
        licence_url: "https://github.com/JaidedAI/EasyOCR/blob/master/LICENSE",
        source: "QVAC model registry OCR_LATIN (qvac_models_compiled/ocr/gguf/easyocr/2026-05-14)",
        default_llm: false,
    },
    CatalogModel {
        id: "qwen2.5-0.5b-instruct-q4_k_m",
        name: "Qwen2.5 0.5B Instruct (Q4_K_M)",
        purpose: ModelPurpose::Llm,
        file_name: "qwen2.5-0.5b-instruct-q4_k_m.gguf",
        url: Some("https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/9217f5db79a29953eb74d5343926648285ec7e67/qwen2.5-0.5b-instruct-q4_k_m.gguf"),
        size: 491_400_032,
        sha256: "74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db",
        licence: "Apache-2.0",
        licence_url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/blob/9217f5db79a29953eb74d5343926648285ec7e67/LICENSE",
        source: "Hugging Face Qwen/Qwen2.5-0.5B-Instruct-GGUF @ 9217f5d",
        default_llm: true,
    },
    CatalogModel {
        id: "qwen2.5-1.5b-instruct-q4_k_m",
        name: "Qwen2.5 1.5B Instruct (Q4_K_M)",
        purpose: ModelPurpose::Llm,
        file_name: "qwen2.5-1.5b-instruct-q4_k_m.gguf",
        url: Some("https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/91cad51170dc346986eccefdc2dd33a9da36ead9/qwen2.5-1.5b-instruct-q4_k_m.gguf"),
        size: 1_117_320_736,
        sha256: "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e",
        licence: "Apache-2.0",
        licence_url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/blob/91cad51170dc346986eccefdc2dd33a9da36ead9/LICENSE",
        source: "Hugging Face Qwen/Qwen2.5-1.5B-Instruct-GGUF @ 91cad51",
        default_llm: false,
    },
];

pub fn find(id: &str) -> Option<&'static CatalogModel> {
    CATALOG.iter().find(|m| m.id == id)
}

pub fn find_by_sha256(sha: &str) -> Option<&'static CatalogModel> {
    CATALOG.iter().find(|m| m.sha256.eq_ignore_ascii_case(sha))
}

pub fn default_llm() -> &'static CatalogModel {
    CATALOG.iter().find(|m| m.default_llm).expect("catalog has a default LLM")
}

pub fn ocr_pair() -> (&'static CatalogModel, &'static CatalogModel) {
    (
        CATALOG.iter().find(|m| m.purpose == ModelPurpose::OcrDetector).expect("detector"),
        CATALOG.iter().find(|m| m.purpose == ModelPurpose::OcrRecognizer).expect("recognizer"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_well_formed() {
        let mut ids = std::collections::HashSet::new();
        for m in CATALOG {
            assert!(ids.insert(m.id), "duplicate id {}", m.id);
            assert_eq!(m.sha256.len(), 64, "{}", m.id);
            assert!(m.sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
            assert!(m.size > 0);
            assert!(m.file_name.ends_with(".gguf"));
            assert!(!m.file_name.contains('/') && !m.file_name.contains('\\'));
            if let Some(url) = m.url {
                assert!(url.starts_with("https://huggingface.co/"), "{} must be HTTPS from Hugging Face", m.id);
                assert!(url.contains("/resolve/") && !url.contains("/resolve/main/"), "{} must pin a revision", m.id);
                assert!(!url.contains("registry://") && !url.contains("pear://"));
            }
        }
        assert_eq!(CATALOG.iter().filter(|m| m.default_llm).count(), 1);
        assert_eq!(default_llm().purpose, ModelPurpose::Llm);
        let (d, r) = ocr_pair();
        assert_eq!(d.file_name, "craft_mlt_25k.gguf");
        assert_eq!(r.file_name, "latin_g2.gguf");
    }
}
