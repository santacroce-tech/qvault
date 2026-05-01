# Introducing Layer 3: Local AI for Contextual PII Detection

**March 26, 2026** · Product Update

Qvault's detection engine now includes a small language model that runs entirely on your device. It catches sensitive information hidden in natural language that patterns alone can't find — and it never transmits a single byte of your data.

## The Problem: PII That Patterns Miss

Regex and heuristic analysis are excellent at catching structured PII: email addresses, phone numbers, SSNs, CNPJs, credit card numbers. They work because these entities follow predictable formats.

But legal documents often contain sensitive information expressed in natural language. Consider this sentence from a real Brazilian hosting contract:

> *"...representada por seu sócio Guilherme Barile."*

There's no email, no ID number, no pattern to match — just a person's name in context. A regex will never find it. A heuristic scanner might catch it if it recognizes "representada por" as a trigger, but what about less common phrasings? What about "my neighbor John lives on Oak Street"?

This is where a language model excels. It understands context, not just patterns.

## How the 3-Layer Pipeline Works

Each layer builds on the previous one. Entities found by earlier layers are excluded from later passes to prevent duplicates.

```
Document text (per page)
  → Layer 1: Regex    (emails, SSNs, phones, dates, amounts...)     95% confidence
  → Layer 2: Heuristic (names, companies, addresses by context)     75–92% confidence
  → Layer 3: LLM      (contextual PII regex/heuristic missed)      70% confidence
  → Combined, deduplicated, sorted by position
```

### Layer 1: Regex Patterns

High-precision pattern matching for structured data. Supports multiple jurisdictions: US (SSN, EIN), EU (IBAN), Brazil (CPF, CNPJ, CEP), Germany, and global patterns (email, phone, credit card, dates, monetary amounts).

- **Confidence:** 95%
- Deterministic, fastest layer

### Layer 2: Heuristic Analysis

Context-aware detection using capitalization patterns, legal role keywords, and company suffixes. Recognizes "Giuliano Ferrari" after "representada por", detects "ELEVADORES ZENIT LTDA" by the company suffix, and filters false positives like "Vila Santa Catarina" using address stop words.

- **Confidence:** 75–92%
- Supports EN, PT-BR, ES legal terminology

### Layer 3: Local AI Model ✨ NEW

A quantized Qwen 2.5 0.5B model powered by [HuggingFace Candle](https://github.com/huggingface/candle) (pure Rust, no C++ dependencies). The model receives each page of text with a structured prompt and returns a JSON array of PII entities it found. Results are mapped back to exact character offsets and filtered against existing detections.

- **Confidence:** 70%
- GPU accelerated (CUDA / Metal)
- ~400 MB download
- Optional — Qvault works perfectly without it

## Why Candle Instead of llama.cpp?

We evaluated several options for local inference: llama.cpp (via `llama-cpp-2`), ONNX Runtime, Burn, and Candle. We chose Candle because:

- **Pure Rust** — no cmake, no C++ compiler needed. Trivial cross-platform builds for macOS, Windows, Linux, and eventually mobile.
- **Simple Tauri integration** — no external binaries or shell plugins. Just another Rust crate in our dependency tree.
- **GPU acceleration** — CUDA for NVIDIA GPUs, Metal for Apple Silicon. Falls back to CPU automatically.
- **Performance is sufficient** — for a 0.5B model, the speed difference versus llama.cpp is negligible.

## Privacy by Design

The AI model follows the same zero-transmission principle as everything else in Qvault:

- The model file (~400 MB) is downloaded once from HuggingFace and cached locally.
- All inference runs on your CPU or GPU — no API calls, no cloud services.
- The feature is entirely optional. Qvault works perfectly with just Layers 1 and 2.
- The model auto-unloads from memory after 30 seconds of inactivity to conserve RAM.

## Brazilian Contract Detection Improvements

Alongside Layer 3, we also significantly improved Brazilian contract detection in Layers 1 and 2:

- **CEP** (postal code) pattern: `00000-000`
- **Company suffixes**: LTDA, EIRELI, ME, EPP, Cia
- **"representada por"** pattern for extracting representative names
- **Brazilian legal roles**: Sócio, Diretor, Gerente, Administrador, Procurador
- **Address stop words**: Rua, Alameda, Avenida, Vila, Bairro (prevents false name detections)

Tested against a real Brazilian cloud hosting contract (Zenit / Bravado), the pipeline now correctly identifies:

| Entity | Category | Confidence | Layer |
|--------|----------|------------|-------|
| ELEVADORES ZENIT LTDA | Company | 90% | Heuristic |
| Bravado Tecnologia da Informação EIRELI | Company | 90% | Heuristic |
| Giuliano Ferrari | Person | 92% | Heuristic |
| Guilherme Barile | Person | 92% | Heuristic |
| 04371-000 | Address (CEP) | 95% | Regex |
| 61.520.011/0001-97 | Tax ID (CNPJ) | 95% | Regex |
| 35.661.121/0001-69 | Tax ID (CNPJ) | 95% | Regex |

## How to Enable Layer 3

1. Open Qvault and go to **Settings**.
2. In the **AI Detection** section, click **Download Model** (~400 MB, one-time).
3. Once downloaded, click **Load Model**. The settings page will show your compute device (CPU, CUDA GPU, or Metal GPU).
4. Enable the toggle. Layer 3 will now run automatically during scans.

The model auto-unloads after 30 seconds of inactivity to free memory. It reloads automatically on the next scan.

---

Qvault is free, open source (MIT), and runs on macOS, Windows, and Linux. [Download it here](https://qvault.app) or check out the [source on GitHub](https://github.com/santacroce-tech/qvault).
