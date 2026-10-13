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
    /// Text embeddings for entity grouping (Phase 4).
    Embedding,
    /// Token classifier for PII (OpenAI Privacy Filter, run through
    /// `@qvac/onnx`; docs/qvac-integration.md §15).
    PiiClassifier,
}

/// One file of a multi-file model.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogFile {
    pub name: &'static str,
    pub url: Option<&'static str>,
    pub size: u64,
    pub sha256: &'static str,
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
    /// For import-only models: a pinned HTTPS page/file the user downloads
    /// from in their own browser, then imports. Opened only through
    /// `ai_open_manual_download`, never fetched by the app itself.
    pub manual_url: Option<&'static str>,
    pub size: u64,
    pub sha256: &'static str,
    pub licence: &'static str,
    pub licence_url: &'static str,
    /// Where the bytes come from, for the UI and ATTRIBUTION.txt.
    pub source: &'static str,
    /// Default Layer 3 model.
    pub default_llm: bool,
    /// Approximate memory needed to run it (bytes; measured peak of the
    /// sidecar during the Phase 3 eval, rounded up). 0 for OCR models.
    pub ram_needed: u64,
    /// Shown in the Model Manager before download.
    pub note: Option<&'static str>,
    /// Multi-file models live in `models/<dir>/` (every file side by side,
    /// as the runtime expects); `None` = one file at `models/<file_name>`.
    pub dir: Option<&'static str>,
    /// The other files of a multi-file model. `file_name`/`url`/`size`/
    /// `sha256` above describe its primary file (the one that is loaded).
    pub extra_files: &'static [CatalogFile],
}

impl CatalogModel {
    /// Every file of the model, the primary one first.
    pub fn files(&self) -> Vec<CatalogFile> {
        let mut v = vec![CatalogFile { name: self.file_name, url: self.url, size: self.size, sha256: self.sha256 }];
        v.extend_from_slice(self.extra_files);
        v
    }

    /// Download / disk size of all files.
    pub fn total_size(&self) -> u64 {
        self.size + self.extra_files.iter().map(|f| f.size).sum::<u64>()
    }

    pub fn is_multi_file(&self) -> bool {
        self.dir.is_some()
    }
}

pub const CATALOG: &[CatalogModel] = &[
    CatalogModel {
        id: "easyocr-craft-mlt-25k",
        name: "CRAFT text detector (EasyOCR, GGUF)",
        purpose: ModelPurpose::OcrDetector,
        file_name: "craft_mlt_25k.gguf",
        // No official HTTPS host exists for QVAC's GGUF conversion (the QVAC
        // registry serves it over Hyperswarm only). Qvault mirrors the files
        // byte-identical on its `ocr-models-v1` GitHub release; users download
        // them in a browser and import them (the app never fetches them).
        url: None,
        manual_url: Some("https://github.com/santacroce-tech/qvault/releases/download/ocr-models-v1/craft_mlt_25k.gguf"),
        size: 83_133_856,
        sha256: "74501993caf4581ce09b280f49a1b3d249c0f5a78496e047718b398457a875aa",
        licence: "MIT (CRAFT weights, NAVER Clova AI); GGUF conversion Apache-2.0 (Tether QVAC)",
        licence_url: "https://github.com/clovaai/CRAFT-pytorch/blob/master/LICENSE",
        source: "QVAC model registry OCR_CRAFT (qvac_models_compiled/ocr/gguf/easyocr/2026-05-14)",
        default_llm: false,
        ram_needed: 0,
        note: None,
        dir: None,
        extra_files: &[],
    },
    CatalogModel {
        id: "easyocr-latin-g2",
        name: "Latin text recognizer (EasyOCR gen-2, GGUF)",
        purpose: ModelPurpose::OcrRecognizer,
        file_name: "latin_g2.gguf",
        url: None,
        manual_url: Some("https://github.com/santacroce-tech/qvault/releases/download/ocr-models-v1/latin_g2.gguf"),
        size: 15_396_512,
        sha256: "dd1c7a436e9175904e63683e939635892f45c8c6377d5e14e87cf8b175e10768",
        licence: "Apache-2.0 (EasyOCR, Jaided AI); GGUF conversion Apache-2.0 (Tether QVAC)",
        licence_url: "https://github.com/JaidedAI/EasyOCR/blob/master/LICENSE",
        source: "QVAC model registry OCR_LATIN (qvac_models_compiled/ocr/gguf/easyocr/2026-05-14)",
        default_llm: false,
        ram_needed: 0,
        note: None,
        dir: None,
        extra_files: &[],
    },
    // Layer 3 LLMs. Chosen on the Phase 3 eval (docs/qvac-integration.md
    // §11.4): official Qwen GGUFs at pinned revisions, SHA-256 computed after
    // download (and equal to the HF LFS object id). Qwen publishes the small
    // Qwen3 models only as Q8_0. Memory needs are the measured peak RSS of the
    // sidecar on the eval, rounded up.
    CatalogModel {
        id: "qwen3-0.6b-q8_0",
        name: "Qwen3 0.6B (Q8_0) — light",
        purpose: ModelPurpose::Llm,
        file_name: "qwen3-0.6b-q8_0.gguf",
        url: Some("https://huggingface.co/Qwen/Qwen3-0.6B-GGUF/resolve/23749fefcc72300e3a2ad315e1317431b06b590a/Qwen3-0.6B-Q8_0.gguf"),
        manual_url: None,
        size: 639_446_688,
        sha256: "9465e63a22add5354d9bb4b99e90117043c7124007664907259bd16d043bb031",
        licence: "Apache-2.0",
        licence_url: "https://huggingface.co/Qwen/Qwen3-0.6B-GGUF/blob/23749fefcc72300e3a2ad315e1317431b06b590a/LICENSE",
        source: "Hugging Face Qwen/Qwen3-0.6B-GGUF @ 23749fe",
        default_llm: false,
        ram_needed: 1_610_612_736, // 1.5 GiB (measured 1.30 GB)
        note: Some("For computers with little memory: finds less than the default model."),
        dir: None,
        extra_files: &[],
    },
    CatalogModel {
        id: "qwen3-1.7b-q8_0",
        name: "Qwen3 1.7B (Q8_0)",
        purpose: ModelPurpose::Llm,
        file_name: "qwen3-1.7b-q8_0.gguf",
        url: Some("https://huggingface.co/Qwen/Qwen3-1.7B-GGUF/resolve/90862c4b9d2787eaed51d12237eafdfe7c5f6077/Qwen3-1.7B-Q8_0.gguf"),
        manual_url: None,
        size: 1_834_426_016,
        sha256: "061b54daade076b5d3362dac252678d17da8c68f07560be70818cace6590cb1a",
        licence: "Apache-2.0",
        licence_url: "https://huggingface.co/Qwen/Qwen3-1.7B-GGUF/blob/90862c4b9d2787eaed51d12237eafdfe7c5f6077/LICENSE",
        source: "Hugging Face Qwen/Qwen3-1.7B-GGUF @ 90862c4",
        default_llm: true,
        ram_needed: 2_952_790_016, // 2.75 GiB (measured 2.43 GB)
        note: None,
        dir: None,
        extra_files: &[],
    },
    CatalogModel {
        id: "qwen3-4b-q4_k_m",
        name: "Qwen3 4B (Q4_K_M) — workstation",
        purpose: ModelPurpose::Llm,
        file_name: "qwen3-4b-q4_k_m.gguf",
        url: Some("https://huggingface.co/Qwen/Qwen3-4B-GGUF/resolve/bc640142c66e1fdd12af0bd68f40445458f3869b/Qwen3-4B-Q4_K_M.gguf"),
        manual_url: None,
        size: 2_497_280_256,
        sha256: "7485fe6f11af29433bc51cab58009521f205840f5b4ae3a32fa7f92e8534fdf5",
        licence: "Apache-2.0",
        licence_url: "https://huggingface.co/Qwen/Qwen3-4B-GGUF/blob/bc640142c66e1fdd12af0bd68f40445458f3869b/LICENSE",
        source: "Hugging Face Qwen/Qwen3-4B-GGUF @ bc64014",
        default_llm: false,
        ram_needed: 3_758_096_384, // 3.5 GiB (measured 3.08 GB)
        note: Some("Finds more, about twice as slow as the default. Best with 16 GB of memory or a GPU."),
        dir: None,
        extra_files: &[],
    },
    CatalogModel {
        id: "qwen3-8b-q4_k_m",
        name: "Qwen3 8B (Q4_K_M) — large workstation",
        purpose: ModelPurpose::Llm,
        file_name: "qwen3-8b-q4_k_m.gguf",
        url: Some("https://huggingface.co/Qwen/Qwen3-8B-GGUF/resolve/7c41481f57cb95916b40956ab2f0b139b296d974/Qwen3-8B-Q4_K_M.gguf"),
        manual_url: None,
        size: 5_027_783_488,
        sha256: "d98cdcbd03e17ce47681435b5150e34c1417f50b5c0019dd560e4882c5745785",
        licence: "Apache-2.0",
        licence_url: "https://huggingface.co/Qwen/Qwen3-8B-GGUF/blob/7c41481f57cb95916b40956ab2f0b139b296d974/LICENSE",
        source: "Hugging Face Qwen/Qwen3-8B-GGUF @ 7c41481",
        default_llm: false,
        ram_needed: 6_442_450_944, // 6 GiB (measured 5.52 GB)
        note: Some("Highest recall in the eval, about 3.5× slower than the default. Needs a GPU workstation."),
        dir: None,
        extra_files: &[],
    },
    CatalogModel {
        id: "qwen2.5-0.5b-instruct-q4_k_m",
        name: "Qwen2.5 0.5B Instruct (Q4_K_M)",
        purpose: ModelPurpose::Llm,
        file_name: "qwen2.5-0.5b-instruct-q4_k_m.gguf",
        url: Some("https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/9217f5db79a29953eb74d5343926648285ec7e67/qwen2.5-0.5b-instruct-q4_k_m.gguf"),
        manual_url: None,
        size: 491_400_032,
        sha256: "74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db",
        licence: "Apache-2.0",
        licence_url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/blob/9217f5db79a29953eb74d5343926648285ec7e67/LICENSE",
        source: "Hugging Face Qwen/Qwen2.5-0.5B-Instruct-GGUF @ 9217f5d",
        default_llm: false,
        ram_needed: 1_073_741_824, // 1 GiB (measured 0.94 GB)
        note: Some("Superseded by Qwen3 0.6B, which finds more and is faster."),
        dir: None,
        extra_files: &[],
    },
    CatalogModel {
        id: "qwen2.5-1.5b-instruct-q4_k_m",
        name: "Qwen2.5 1.5B Instruct (Q4_K_M)",
        purpose: ModelPurpose::Llm,
        file_name: "qwen2.5-1.5b-instruct-q4_k_m.gguf",
        url: Some("https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/91cad51170dc346986eccefdc2dd33a9da36ead9/qwen2.5-1.5b-instruct-q4_k_m.gguf"),
        manual_url: None,
        size: 1_117_320_736,
        sha256: "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e",
        licence: "Apache-2.0",
        licence_url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/blob/91cad51170dc346986eccefdc2dd33a9da36ead9/LICENSE",
        source: "Hugging Face Qwen/Qwen2.5-1.5B-Instruct-GGUF @ 91cad51",
        default_llm: false,
        ram_needed: 1_879_048_192, // 1.75 GiB (measured 1.56 GB)
        note: Some("Superseded by Qwen3 1.7B (more precise suggestions)."),
        dir: None,
        extra_files: &[],
    },
    // Entity grouping (Phase 4, docs/qvac-integration.md §12). Official GGUF
    // from Nomic AI at a pinned revision; SHA-256 computed after download
    // (equal to the HF LFS object id). Chosen over Qwen3-Embedding-0.6B,
    // which needed ~5 GB of memory on the pinned addon (its 32k context
    // cannot be lowered through @qvac/embed-llamacpp 0.43.0's config) for no
    // better result. Optional: grouping works without it.
    CatalogModel {
        id: "nomic-embed-text-v2-moe-q8_0",
        name: "Nomic Embed Text v2 MoE (Q8_0)",
        purpose: ModelPurpose::Embedding,
        file_name: "nomic-embed-text-v2-moe.Q8_0.gguf",
        url: Some("https://huggingface.co/nomic-ai/nomic-embed-text-v2-moe-GGUF/resolve/ffbcf4c99e5d617dda10ec8c0e9f75754b0cbb80/nomic-embed-text-v2-moe.Q8_0.gguf"),
        manual_url: None,
        size: 512_225_120,
        sha256: "06e7a7e594a26985523c18383aba4aad39fe6e14f08ffc6ab5b554e1ccdc3cff",
        licence: "Apache-2.0",
        licence_url: "https://huggingface.co/nomic-ai/nomic-embed-text-v2-moe-GGUF/blob/ffbcf4c99e5d617dda10ec8c0e9f75754b0cbb80/README.md",
        source: "Hugging Face nomic-ai/nomic-embed-text-v2-moe-GGUF @ ffbcf4c",
        default_llm: false,
        ram_needed: 1_879_048_192, // 1.75 GiB (measured 1.62 GB)
        note: Some("Optional. Adds similarity-based suggestions when grouping name variants; grouping works without it."),
        dir: None,
        extra_files: &[],
    },
    // Layer 3 PII detector (docs/qvac-integration.md §15): OpenAI Privacy
    // Filter, int8 ONNX (`model_quantized`; the q4 files lost 6.7 points of
    // held-out recall, §14). Four files from the original author at a pinned
    // revision; sizes and SHA-256 equal the Hub's LFS metadata (the small
    // calibration file is not in LFS: hash computed after download).
    CatalogModel {
        id: "openai-privacy-filter-int8",
        name: "OpenAI Privacy Filter (int8)",
        purpose: ModelPurpose::PiiClassifier,
        file_name: "model_quantized.onnx",
        url: Some("https://huggingface.co/openai/privacy-filter/resolve/7ffa9a043d54d1be65afb281eddf0ffbe629385b/onnx/model_quantized.onnx"),
        manual_url: None,
        size: 162_239,
        sha256: "a325fb5341567a73c94e91ec5e49060d38d9b16111f518ad34773039a0c9c098",
        licence: "Apache-2.0",
        licence_url: "https://huggingface.co/openai/privacy-filter/blob/7ffa9a043d54d1be65afb281eddf0ffbe629385b/README.md",
        source: "Hugging Face openai/privacy-filter @ 7ffa9a0",
        default_llm: false,
        ram_needed: 2_415_919_104, // 2.25 GiB (measured 1.9 GB in the sidecar)
        note: Some("Fast on any computer (well under a second per page without a GPU). Finds names, addresses, dates, contact details and account-style numbers; company names, amounts and case numbers stay with the rules (and the language model, if you also run it)."),
        dir: Some("openai-privacy-filter-int8"),
        extra_files: &[
            CatalogFile {
                name: "model_quantized.onnx_data",
                url: Some("https://huggingface.co/openai/privacy-filter/resolve/7ffa9a043d54d1be65afb281eddf0ffbe629385b/onnx/model_quantized.onnx_data"),
                size: 1_618_042_064,
                sha256: "50f4c8c7f3c27fbc1fe16d4f74f6f7c3b74ba8f18a262e8b6911854c64c33a6d",
            },
            CatalogFile {
                name: "tokenizer.json",
                url: Some("https://huggingface.co/openai/privacy-filter/resolve/7ffa9a043d54d1be65afb281eddf0ffbe629385b/tokenizer.json"),
                size: 27_868_174,
                sha256: "0614fe83cadab421296e664e1f48f4261fa8fef6e03e63bb75c20f38e37d07d3",
            },
            CatalogFile {
                name: "viterbi_calibration.json",
                url: Some("https://huggingface.co/openai/privacy-filter/resolve/7ffa9a043d54d1be65afb281eddf0ffbe629385b/viterbi_calibration.json"),
                size: 372,
                sha256: "bbc8611ef08a55ed72d64856cbbbb9a91db8dfa881f0a92e2afbad6e4bbc775a",
            },
        ],
    },
];

pub fn find(id: &str) -> Option<&'static CatalogModel> {
    CATALOG.iter().find(|m| m.id == id)
}

/// A single-file catalog model whose file has this SHA-256.
pub fn find_by_sha256(sha: &str) -> Option<&'static CatalogModel> {
    CATALOG.iter().find(|m| !m.is_multi_file() && m.sha256.eq_ignore_ascii_case(sha))
}

/// A file of a multi-file catalog model with this SHA-256.
pub fn find_part_by_sha256(sha: &str) -> Option<(&'static CatalogModel, CatalogFile)> {
    CATALOG
        .iter()
        .filter(|m| m.is_multi_file())
        .find_map(|m| m.files().into_iter().find(|f| f.sha256.eq_ignore_ascii_case(sha)).map(|f| (m, f)))
}

/// Whether some catalog file has exactly this size (cheap pre-filter before
/// hashing the files of a folder import).
pub fn has_file_of_size(size: u64) -> bool {
    CATALOG.iter().any(|m| m.files().iter().any(|f| f.size == size))
}

/// The Privacy Filter catalog entry.
pub fn privacy_filter() -> &'static CatalogModel {
    CATALOG.iter().find(|m| m.purpose == ModelPurpose::PiiClassifier).expect("catalog has the Privacy Filter")
}

pub fn default_llm() -> &'static CatalogModel {
    CATALOG.iter().find(|m| m.default_llm).expect("catalog has a default LLM")
}

/// The catalog embedding model (entity grouping).
pub fn embedding_model() -> &'static CatalogModel {
    CATALOG.iter().find(|m| m.purpose == ModelPurpose::Embedding).expect("catalog has an embedding model")
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
            if m.is_multi_file() {
                let dir = m.dir.unwrap();
                assert!(!dir.is_empty() && !dir.contains('/') && !dir.contains('\\') && !dir.starts_with('.'));
                let mut names = std::collections::HashSet::new();
                for f in m.files() {
                    assert!(names.insert(f.name), "{}: duplicate file {}", m.id, f.name);
                    assert!(!f.name.contains('/') && !f.name.contains('\\') && !f.name.starts_with('.'));
                    assert_eq!(f.sha256.len(), 64);
                    assert!(f.size > 0);
                    assert_eq!(f.url.is_some(), m.url.is_some(), "{}: all files downloadable or none", m.id);
                }
            } else {
                assert!(m.file_name.ends_with(".gguf"));
                assert!(m.extra_files.is_empty());
            }
            assert!(!m.file_name.contains('/') && !m.file_name.contains('\\'));
            for url in m.files().iter().filter_map(|f| f.url) {
                assert!(url.starts_with("https://huggingface.co/"), "{} must be HTTPS from Hugging Face", m.id);
                assert!(url.contains("/resolve/") && !url.contains("/resolve/main/"), "{} must pin a revision", m.id);
                assert!(!url.contains("registry://") && !url.contains("pear://"));
            }
            if let Some(manual) = m.manual_url {
                assert!(m.url.is_none(), "{}: manual_url is only for import-only models", m.id);
                let prefix = "https://github.com/santacroce-tech/qvault/releases/download/ocr-models-v1/";
                assert!(manual.starts_with(prefix), "{} manual_url must point at the pinned release", m.id);
                assert!(manual.ends_with(m.file_name), "{} manual_url must name the catalog file", m.id);
            }
        }
        assert_eq!(CATALOG.iter().filter(|m| m.default_llm).count(), 1);
        assert_eq!(default_llm().purpose, ModelPurpose::Llm);
        assert_eq!(embedding_model().purpose, ModelPurpose::Embedding);
        let (d, r) = ocr_pair();
        assert_eq!(d.file_name, "craft_mlt_25k.gguf");
        assert_eq!(r.file_name, "latin_g2.gguf");
        let pf = privacy_filter();
        assert_eq!(pf.files().len(), 4);
        assert_eq!(pf.total_size(), 162_239 + 1_618_042_064 + 27_868_174 + 372);
        assert!(pf.files().iter().all(|f| f.url.unwrap().contains("/7ffa9a043d54d1be65afb281eddf0ffbe629385b/")));
        assert_eq!(find_part_by_sha256(&pf.extra_files[1].sha256.to_uppercase()).map(|(m, f)| (m.id, f.name)), Some((pf.id, "tokenizer.json")));
        assert!(find_by_sha256(pf.sha256).is_none(), "parts of a multi-file model are not single-file models");
        // every hash is unique across the catalog (imports are matched by hash)
        let mut hashes = std::collections::HashSet::new();
        for m in CATALOG {
            for f in m.files() {
                assert!(hashes.insert(f.sha256), "duplicate hash {}", f.sha256);
            }
        }
    }
}
