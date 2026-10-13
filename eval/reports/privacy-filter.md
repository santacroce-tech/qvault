# Eval: OpenAI Privacy Filter as a Layer 3 candidate (measurement only)

Offline measurement of [`openai/privacy-filter`](https://huggingface.co/openai/privacy-filter) on Qvault's 16 eval fixtures. Nothing was integrated into the app and app behaviour is unchanged. The only code added to the repository is the opt-in harness path `QVAULT_EVAL_EXTERNAL` (`src-tauri/tests/eval_support/external.rs`), which scores predictions made outside Qvault with the harness's own scoring. Numbers: [`privacy-filter.json`](privacy-filter.json). Decision summary: [docs/qvac-integration.md §14](../../docs/qvac-integration.md#14-openai-privacy-filter-measured-not-integrated).

## Summary

- **Quality is about Qwen3-1.7B's (the current default), with a different profile.**
  - Layers 1+2 plus Privacy Filter (int8 ONNX, default operating point) score **83.0 / 96.3 / 89.2** (P / R / F1). Layers 1+2 plus Qwen3-1.7B score 84.9 / 93.1 / 88.8.
  - Privacy Filter has higher recall (+3.2), higher covered recall (88.3 vs 85.0) and higher held-out recall (90.0 vs 85.0).
  - It has lower precision: alone 93.1 vs 97.2, and 23 vs 8 model false positives in the combined output.
  - It is weaker in Portuguese (combined F1 89.0 vs 92.5).
  - Gemma 4 E4B is still the best (91.3 F1).
- **It is 30–75× faster on CPU than Qwen3-1.7B is on the Metal GPU:** 0.08–0.19 s/page against 6.2 s/page, with about 1.3–2.4 GB of memory.
- **It cannot see several Qvault categories.** It has no labels for company names, money amounts, or case and court-file numbers. National IDs, tax IDs, passports, plates and similar are only caught as `account_number`, so type accuracy is 74 % (Qwen3-1.7B: 93 %).
- **Use the int8 file, not q4.** q4 loses 1 point of overall recall and 6.7 points of held-out recall (90.0 → 83.3). int8 matches the full-precision reference and is also the fastest on CPU.
- **Runtime:**
  - ONNX Runtime runs it identically from Rust (`ort`, +17.9 MB binary) and from **QVAC's own `@qvac/onnx` addon inside the existing Bare sidecar**, which was verified (+20.7 MB per platform).
  - Transformers.js in WKWebView works only on WebGPU (q4f16, 0.51 s/page, 5.2 GB in WebKit processes). The WASM backend cannot load either quantized file.
- **Recommendation: do not replace Qwen3-1.7B. Treat Privacy Filter as the strongest candidate for a fast, CPU-friendly complement,** to be prototyped through `@qvac/onnx` in the existing sidecar if and when CPU-only speed becomes a priority. Details at the end.

## Method

- **Model:** `openai/privacy-filter` at revision `7ffa9a043d54d1be65afb281eddf0ffbe629385b`, Apache-2.0. Every file's SHA-256 was computed after download and matches the Hub's LFS object id:

  | file | SHA-256 |
  |---|---|
  | `original/model.safetensors` (2.80 GB, opf reference checkpoint) | `9c262cbe68a0c8a50590a648ef8341a2b7d3be1fa11dfb79893fe0b03ce57b5c` |
  | `original/config.json` | `048a20604a3622de208d30df57cd5424bb583639b9ba20ddd7da593d3f89a248` |
  | `model.safetensors` (2.80 GB, Transformers) | `06f66b87650b988b04e218285f9fe3df6a4943416b6ffa8171f07bc56cf12a9d` |
  | `config.json` | `b2b26a4a4a000639ad30b0c264adbefe365bdb567fbd7bb27303b8c438375bd1` |
  | `onnx/model_quantized.onnx` + `_data` (1.62 GB, int8) | `a325fb53…c098` + `50f4c8c7f3c27fbc1fe16d4f74f6f7c3b74ba8f18a262e8b6911854c64c33a6d` |
  | `onnx/model_q4.onnx` + `_data` (0.92 GB) | `8f7dee8b…8dea` + `f30998e28c71c5374cc7e8b7de8f0f83e981592c0c2d652d2ad4928454dbb496` |
  | `onnx/model_q4f16.onnx` + `_data` (0.81 GB) | `eaae4e83…5e45` + `6d4dde787e03ace283c45d4e32a94eec32b6cfcc242e7219bea96f5b4c13569d` |
  | `tokenizer.json` | `0614fe83cadab421296e664e1f48f4261fa8fef6e03e63bb75c20f38e37d07d3` |
  | `viterbi_calibration.json` (= `original/…`) | `bbc8611ef08a55ed72d64856cbbbb9a91db8dfa881f0a92e2afbad6e4bbc775a` |

  Full hashes are in the JSON. The fp32 ONNX (5.6 GB) was not used, because the full-precision reference is the bf16 checkpoint run by OpenAI's own code.
- **Reference code:** [`openai/privacy-filter`](https://github.com/openai/privacy-filter) at commit `f7f00ca7fb869683eb732c010299d901457f19c3` (tarball SHA-256 `c79c613e…be39`), installed as the `opf` package. Every backend decodes with opf's own `ViterbiCRFDecoder`, `labels_to_spans`, whitespace trimming and non-overlapping span selection, exactly as `opf.predict_text` does. Tokenisation uses `tiktoken` `o200k_base`, as in opf; `tokenizer.json` gives the same ids. opf's CPU window is 4,096 tokens, and the longest page is 878 tokens, so every page is one window.
- **Backends:**
  1. **opf reference:** OpenAI's PyTorch model on `original/` (bf16, CPU).
  2. **ONNX Runtime** 1.31.0 on the int8, q4 and q4f16 files: CPU, plus the CoreML EP for q4.
  3. **Transformers** 5.19.0 `pipeline("token-classification")` with `aggregation_strategy="simple"`, as on the model card. This is per-token argmax with no Viterbi.
  - Python 3.12.14, venv with pinned packages (`torch 2.14.1`, `onnxruntime 1.31.0`, `transformers 5.19.0`, `tokenizers 0.23.3`, `tiktoken 0.14.0`). Python ran with `-I`, and `HF_HUB_OFFLINE=1`.
  - ONNX CPU results were identical across repeated passes.
- **Input:** the page texts of `eval/fixtures/*.json`, read directly, so the text is byte for byte what the harness scores. Character spans were converted to UTF-8 byte offsets. The harness rejects any span whose byte slice differs from the predicted text, and no span was rejected.
- **Categories:**
  - `private_person` → PersonName, `private_email` → Email, `private_phone` → PhoneNumber, `private_address` → Address, `private_url` → Url, `private_date` → Date, `account_number` → BankAccount, `secret` → ClientId (`secret` was never predicted).
  - Matching is category-agnostic, so the mapping only affects type accuracy and the per-category precision columns.
- **Operating points:**
  - The published `viterbi_calibration.json` contains **only `default`** (all six transition biases 0), and opf's loader accepts only that key. There is no published recall preset.
  - The extra points below were set by hand with opf's bias parameters. **`stay3`** (background-stay bias −3) is reported as the recall point; it was chosen as the best Layer 3-alone F1 on the **dev** split only. `stay4` is the most recall-leaning point measured.
- **Scoring:** the harness, unchanged.
  - Overlap is the headline; exact and covered recall are reported too. Optional labels are ignored. See `eval/README.md`.
  - "L1+2+3" adds the model's spans to Layers 1+2 the way the app adds Layer 3: a span that overlaps a Layer 1–2 detection is dropped.
  - "L1 + L3" uses Layer 1 without Layer 2's name heuristic.
  - Qwen3-1.7B and Gemma 4 E4B numbers are from §11.4 (`eval/reports/layer3-*.md`). Layers 1+2 are unchanged since then (CI baseline).

```sh
# score predictions produced elsewhere (JSON with byte offsets; format in external.rs)
QVAULT_EVAL_EXTERNAL=/abs/pf-onnx-int8-default.json,/abs/other.json QVAULT_EVAL_OUT=/abs/out \
  cargo test --test eval_harness external_predictions -- --nocapture
```

## Results (461 labels, overlap matching)

| system | L3 alone P / R / F1 | L3 cov R | L1+2+3 P / R / F1 | L1+2+3 cov R | L1+2+3 F1 EN / ES / PT | holdout L1+2+3 R / F1 | holdout L3 alone R | L1 + L3 (no L2) P / R / F1 |
|---|---|---:|---|---:|---|---|---:|---|
| Layers 1+2 only | — | — | 84.9 / 83.1 / 84.0 | 75.5 | 80.0 / 88.2 / 90.2 | 66.7 / 75.4 | — | — |
| **Qwen3-1.7B Q8_0** (default, §11.4) | **97.2 / 67.0 / 79.3** | 66.2 | **84.9 / 93.1 / 88.8** | 85.0 | 87.1 / 89.7 / 92.5 | 85.0 / 87.1 | — | — |
| Gemma 4 E4B Q4_K_M (§11.4) | 99.5 / 89.2 / 94.1 | 88.7 | 86.4 / 96.7 / 91.3 | 88.7 | 89.0 / 93.8 / 94.8 | 88.3 / 89.0 | — | — |
| PF opf reference, PyTorch bf16, default | 93.0 / 66.4 / 77.5 | 62.0 | 83.0 / 96.3 / 89.2 | 88.3 | 88.7 / 90.7 / 89.0 | 90.0 / 87.7 | 75.0 | 94.6 / 90.7 / 92.6 |
| PF opf reference, bf16, recall (`stay3`) | 90.3 / 75.1 / 82.0 | 70.5 | 81.3 / 97.0 / 88.4 | 89.6 | 88.3 / 89.6 / 87.7 | 93.3 / 89.0 | 88.3 | 91.8 / 93.1 / 92.4 |
| **PF ONNX int8, default** | **93.1 / 67.5 / 78.2** | 63.3 | **83.0 / 96.3 / 89.2** | 88.3 | 88.7 / 90.7 / 89.0 | 90.0 / 87.7 | 76.7 | 94.6 / 90.9 / 92.7 |
| PF ONNX int8, recall (`stay3`) | 90.3 / 76.1 / 82.6 | 71.8 | 81.2 / 97.2 / 88.5 | 90.0 | 88.5 / 89.2 / 87.8 | 95.0 / 89.9 | 91.7 | 91.6 / 93.3 / 92.4 |
| PF ONNX q4, default | 93.2 / 65.5 / 76.9 | 62.0 | 83.0 / 95.2 / 88.7 | 87.4 | 88.6 / 89.9 / 87.9 | 83.3 / 83.9 | 65.0 | 94.7 / 89.6 / 92.1 |
| PF ONNX q4, recall (`stay3`) | 88.7 / 75.1 / 81.3 | 71.6 | 80.0 / 96.3 / 87.4 | 88.9 | 87.7 / 88.2 / 85.9 | 90.0 / 86.4 | 85.0 | 90.3 / 92.2 / 91.3 |
| PF ONNX q4f16, default | 93.2 / 65.5 / 76.9 | 62.0 | 83.0 / 95.2 / 88.7 | 87.4 | 88.6 / 89.9 / 87.9 | 83.3 / 83.9 | 65.0 | 94.7 / 89.6 / 92.1 |
| PF Transformers pipeline, `simple` (bf16, argmax) | 92.1 / 68.3 / 78.5 | 61.0 | 81.2 / 97.2 / 88.5 | 88.5 | 89.3 / 87.5 / 87.2 | 91.7 / 89.1 | 78.3 | 90.7 / 91.1 / 90.9 |

Observations:

- **Against Qwen3-1.7B (int8, default):**
  - Combined recall is +3.2 and combined F1 +0.4. Covered recall is +3.3, so fewer partially visible names.
  - Held-out recall is +5.0. F1 per language: EN +1.6, ES +1.0, **PT −3.5**.
  - Alone, Privacy Filter is 4 points less precise. In the combined output it adds 23 false positives that the reviewer has to reject, against 8 for Qwen3-1.7B. Examples: "AL JUZGADO", "danos morais", "Cartão de Cidadão", "IMI de 2024", "394 LEC", a street named as a place ("Cedofeita"), and given names alone ("Thomas", "Sue").
  - Exact-boundary recall alone is low (52.5 vs Qwen3-1.7B's 61.4) because spans include honorifics ("Ms. Holloway") that the labels exclude. Overlap and covered recall are not affected.
- **Against Gemma 4 E4B:** Gemma is better on every column except held-out recall (PF int8 `stay3` 95.0 vs 88.3) and speed.
- **Layer 1 + Privacy Filter without Layer 2 reaches F1 92.7** (precision 94.6), the highest F1 of any combination measured (Gemma 4 E4B alone reaches 94.1), because Layer 2's name heuristic is the main source of false positives (§11.4). Its recall (90.9) is 5.4 points below Layers 1+2+PF, and recall is what matters for redaction. Layer 2 should stay unless a model replaces it everywhere.
- **Quantisation:**
  - int8 matches the bf16 reference (identical combined numbers; alone ±1 point).
  - q4 and q4f16 lose 1.1 points of combined recall and **6.7 points of held-out recall** (90.0 → 83.3; alone 75.0 → 65.0). The loss is concentrated in the held-out documents.
  - q4f16 gives the same spans as q4 (same 4-bit weights).
- **Viterbi vs argmax:** with the reference model, per-token argmax (`argmax` row below) has slightly higher recall and lower precision than Viterbi `default`. Viterbi's value is in span coherence, not recall.
- **The Transformers pipeline with `simple` aggregation fragments spans.** It produced 738 pieces where opf produced 350, for example "Marcus T. Feen" + "ey" and "April 9, 198" + "6": simple aggregation does not understand the E-/S- tags. Overlap scoring hides this, but a reviewer would see twice as many items, and the pieces of a name are counted as separate detections. The Python pipeline is not usable as is; Transformers.js `simple` returned whole spans (below).

### Operating points

The only published point is `default`. Hand-set biases:

| operating point (biases) | backend | L3 alone P / R / F1 | dev L3 alone F1 | L1+2+3 P / R / F1 | L1+2+3 cov R | holdout L1+2+3 R |
|---|---|---|---:|---|---:|---:|
| `default`: all 0 (published) | opf bf16 | 93.0 / 66.4 / 77.5 | 76.5 | 83.0 / 96.3 / 89.2 | 88.3 | 90.0 |
| `argmax`: no Viterbi | opf bf16 | 91.9 / 68.8 / 78.7 | 77.4 | 82.0 / 96.5 / 88.7 | 88.5 | 91.7 |
| `prec2`: background→start −2 | opf bf16 | 93.8 / 65.5 / 77.1 | 76.5 | 83.3 / 95.9 / 89.1 | 88.1 | 86.7 |
| `start2`: background→start +2 | opf bf16 | 93.0 / 67.5 / 78.2 | 77.0 | 83.0 / 96.3 / 89.1 | 88.3 | 90.0 |
| `stay1`: background stay −1 | opf bf16 | 93.1 / 69.4 / 79.5 | 78.4 | 82.9 / 96.5 / 89.2 | 88.5 | 91.7 |
| `stay2`: background stay −2 | opf bf16 | 91.1 / 72.0 / 80.5 | 79.3 | 82.0 / 97.0 / 88.8 | 89.2 | 93.3 |
| **`stay3`: background stay −3** | opf bf16 | 90.3 / 75.1 / 82.0 | **80.6** | 81.3 / 97.0 / 88.4 | 89.6 | 93.3 |
| `stay4`: background stay −4 | opf bf16 | 85.7 / 77.4 / 81.4 | 79.7 | 78.1 / 97.2 / 86.6 | 90.0 | 95.0 |
| `default` | ONNX int8 | 93.1 / 67.5 / 78.2 | 77.3 | 83.0 / 96.3 / 89.2 | 88.3 | 90.0 |
| `stay2` | ONNX int8 | 91.4 / 71.8 / 80.4 | 79.2 | 82.0 / 97.0 / 88.8 | 89.2 | 93.3 |
| **`stay3`** | ONNX int8 | 90.3 / 76.1 / 82.6 | **81.0** | 81.2 / 97.2 / 88.5 | 90.0 | 95.0 |
| `stay4` | ONNX int8 | 85.5 / 78.3 / 81.8 | 80.2 | 77.9 / 97.4 / 86.6 | 90.2 | 95.0 |
| `default` | ONNX q4 | 93.2 / 65.5 / 76.9 | 77.0 | 83.0 / 95.2 / 88.7 | 87.4 | 83.3 |
| **`stay3`** | ONNX q4 | 88.7 / 75.1 / 81.3 | 80.3 | 80.0 / 96.3 / 87.4 | 88.9 | 90.0 |
| `stay4` | ONNX q4 | 83.5 / 78.7 / 81.1 | 80.1 | 76.6 / 97.0 / 85.6 | 89.6 | 93.3 |

Raising span entry (`start*`) barely moves anything. Penalising background tokens (`stay*`) trades precision for recall smoothly: the model alone goes from 67.5 to 76.1 recall at −2.8 precision, and the held-out set gains most. In combination with Layers 1+2, `default` has the best F1 and `stay3` the best recall.

### By category (overlap recall; precision is in the per-run reports)

| Qvault category | labels | PF label | L1+2 R | PF bf16 alone R | PF int8 alone R | PF q4 alone R | PF int8 `stay3` alone R | L1+2+PF int8 R |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| PersonName | 113 | private_person | 54.0 | 93.8 | 93.8 | 89.4 | 95.6 | 95.6 |
| Date | 72 | private_date | 91.7 | 63.9 | 66.7 | 63.9 | 77.8 | 97.2 |
| DateOfBirth | 9 | private_date | 88.9 | 100.0 | 100.0 | 88.9 | 100.0 | 100.0 |
| Address | 29 | private_address | 89.7 | 79.3 | 79.3 | 72.4 | 86.2 | 96.6 |
| Email | 25 | private_email | 96.0 | 96.0 | 96.0 | 100.0 | 100.0 | 100.0 |
| PhoneNumber | 25 | private_phone | 100.0 | 84.0 | 84.0 | 88.0 | 92.0 | 100.0 |
| Url | 3 | private_url | 100.0 | 66.7 | 66.7 | 66.7 | 100.0 | 100.0 |
| BankAccount | 15 | account_number | 73.3 | 53.3 | 53.3 | 53.3 | 73.3 | 100.0 |
| IBAN | 9 | account_number | 100.0 | 77.8 | 77.8 | 66.7 | 77.8 | 100.0 |
| CreditCard | 8 | account_number | 100.0 | 62.5 | 75.0 | 50.0 | 87.5 | 100.0 |
| NationalId | 17 | none (caught as account_number) | 88.2 | 94.1 | 94.1 | 82.4 | 94.1 | 100.0 |
| TaxId | 16 | none (as account_number) | 100.0 | 56.2 | 56.2 | 50.0 | 75.0 | 100.0 |
| PassportNumber / SSN / MRN / DriversLicense | 3 / 3 / 4 / 4 | none (as account_number) | 100.0 | 75–100 | 75–100 | 100.0 | 100.0 | 100.0 |
| IpAddress | 6 | none (as account_number) | 100.0 | 83.3 | 100.0 | 100.0 | 100.0 | 100.0 |
| LicensePlate | 5 | none (as account_number) | 100.0 | 40.0 | 40.0 | 40.0 | 80.0 | 100.0 |
| ClientId | 5 | none (as account_number) | 80.0 | 60.0 | 60.0 | 80.0 | 100.0 | 80.0 |
| **CompanyName** | 37 | **none** | 81.1 | 16.2 | 18.9 | 21.6 | 35.1 | 81.1 |
| **MonetaryAmount** | 46 | **none** | 100.0 | 0.0 | 0.0 | 0.0 | 6.5 | 100.0 |
| **CaseNumber** | 6 | **none** | 83.3 | 16.7 | 16.7 | 33.3 | 50.0 | 83.3 |
| **CourtFileNumber** | 1 | **none** | 100.0 | 0.0 | 0.0 | 100.0 | 100.0 | 100.0 |

- **Not covered by the taxonomy:**
  - company names. Privacy Filter adds nothing to Layers 1+2 here (81.1 → 81.1), while Qwen3-1.7B raises them to 91.9. The model card says it leaves organisations alone by design.
  - money amounts and case or court-file numbers.
  - Qvault's 23 categories collapse into 8. Every identifier (national ID, tax ID, passport, licence, plate, IP address, client number) comes back as `account_number`. These numbers are usually still found, but they are labelled as bank accounts: type accuracy alone is 74 % against 93 % for Qwen3-1.7B. In the app the replacement label would be wrong unless Layer 1 already typed the span.
- **Where it is strong:** person names. Alone it finds 94 % of them; Layer 2 finds 54 % and Qwen3-1.7B alone 70 %. Most of its combined gain over Qwen3-1.7B comes from names Qwen3-1.7B missed ("Ellen K. Marsh", "Joost van der Linde", "Koldo Iturbe Larrañaga", "Álvaro Sendra Peiró", names split across line breaks). It also finds held-out dates in formats Layer 1 was deliberately not taught ("2025.03.18", "Feb. 3, 2021").
- **Complement:**
  - Labels missed (overlap) by Layers 1+2+Qwen3-1.7B: 30; by Layers 1+2+PF int8: 16; by both: 7. Running both would give an overlap recall of about **98.5 %**, at the cost of both models' false positives (8 + 23, one shared).
  - Qwen3-1.7B finds labels that Privacy Filter misses: 4 company names, 2 Portuguese abbreviated dates ("10 fev. 2025"), a client number and 2 surnames used alone.
  - Missed by both: an Amsterdam address, a German case number, 2 companies, and 3 names ("Felix Brandauer", "Ramírez de la Garza", "J. M. T. Fonseca").

## Speed and memory (Apple M4 Pro, 48 GB, macOS 26.6.2)

18 pages, about 626 tokens per page on average (878 at most). The time is the forward pass per page, warm. Viterbi decoding adds 0.02–0.03 s per page in Python.

| runtime | file | load | s/page | peak memory | notes |
|---|---|---:|---:|---:|---|
| Python, opf reference PyTorch (CPU) | `original/` bf16, 2.80 GB | 1.2 s | 2.79 | 4.31 GB RSS ¹ | reference only |
| Python, Transformers pipeline (CPU) | bf16, 2.80 GB | 5.2 s | 2.59 | 3.55 GB RSS ¹ | fragments spans |
| Python, onnxruntime CPU | int8, 1.62 GB | 0.5 s | **0.081** | 2.70 GB RSS ¹ | |
| Python, onnxruntime CPU | q4, 0.92 GB | 0.3 s | 0.168 | 1.73 GB RSS ¹ | |
| Python, onnxruntime CPU | q4f16, 0.81 GB | 0.3 s | 0.197 | 1.76 GB RSS ¹ | |
| Python, onnxruntime **CoreML EP** | q4 | 2.6 s | 0.480 | 5.43 GB RSS ¹ | slower: partial CoreML coverage |
| **Rust `ort` 2.0.0-rc.13** (static ORT), CPU | q4 | 1.0 s | 0.211 | 2.36 GB RSS, 2.13 GB footprint | argmax identical to Python on all 11,271 tokens |
| Rust `ort`, CPU | int8 | 0.04 s | 0.183 | 2.36 GB RSS, 0.86 GB footprint | |
| **Bare 1.34.1 + `@qvac/onnx` 0.15.1**, CPU | q4 | 0.03 s | 0.193 | 1.28 GB RSS | argmax identical to Python on all 11,271 tokens |
| Bare + `@qvac/onnx`, CPU | int8 | 0.03 s | 0.194 | 1.92 GB RSS | |
| Bare + `@qvac/onnx`, CoreML | q4 | 0.6 s | 0.337 | 4.20 GB RSS | |
| WKWebView + Transformers.js 4.3.0, **WebGPU** | q4f16 | 0.9 s | **0.51** | 5.2 GB in WebKit processes ² | |
| WKWebView + Transformers.js, WebGPU | q4 | 1.0 s | 20.8 | — | 40× slower than q4f16; cause not investigated |
| WKWebView + Transformers.js, WASM | q4 / int8 | — | — | — | **fails:** `GatherBlockQuantized` has no WASM kernel |
| *Qwen3-1.7B Q8_0, QVAC sidecar, Metal (§11.4)* | 1.83 GB | 1.3 s | *6.22* | *2.43 GB RSS* | GPU; CPU-only not measured |
| *Gemma 4 E4B Q4_K_M, Metal (§11.4)* | 5.41 GB | — | *15.45* | *5.75 GB RSS* | |

¹ Includes the Python interpreter, torch and numpy (≈ 0.2 GB before loading); RSS counts memory-mapped weight pages.
² Peak RSS summed over the WebContent (4.1 GB), Networking (0.9 GB) and GPU (0.2 GB) processes the test spawned.

On CPU, Privacy Filter takes 0.08–0.2 s per page. Qwen3-1.7B takes 6.2 s per page on the GPU, and §11.4 notes that CPU-only machines would be several times slower. That is the one large advantage. The CoreML EP does not help. It is slower and uses more memory, probably because the MoE and 4-bit ops are not supported by CoreML and run on the CPU with copies in between; this was not profiled.

## Integration options (assessed, not built)

Constraints: no network for document data, no silent downloads, models imported from a local file, human review mandatory, and no second AI runtime without a clear reason.

| option | runtime / size | evidence | assessment |
|---|---|---|---|
| **(a) ONNX Runtime from Rust (`ort`)** | `ort 2.0.0-rc.13`, statically linked ORT: release binary **+17.9 MB** (18.31 MB vs 0.42 MB baseline, stripped; +6.8 MB gzip -9), links CoreML.framework | Ran q4 and int8; argmax identical to Python ORT; 0.18–0.21 s/page | Works. It is the same ~17 MB that Phase 1B removed with GLiNER (§6). It makes a **second runtime in the main process**, beside the QVAC sidecar. Rust also needs an o200k tokenizer (`tiktoken-rs`, or `tokenizers` with the 27.9 MB `tokenizer.json`) and a port of opf's Viterbi (about 150 lines). `download-binaries` fetches ORT from pyke's CDN **at build time**; to make the build reproducible, vendor ORT through `ORT_LIB_LOCATION`. |
| **(b) Transformers.js in the Tauri webview** | `@huggingface/transformers` 4.3.0 + onnxruntime-web WASM (14–28 MB per variant) | WKWebView on macOS 26: `navigator.gpu` present, adapter found, cross-origin isolated (COOP/COEP). **WebGPU q4f16: 0.51 s/page; WASM: cannot create a session for q4 or int8** (`GatherBlockQuantized`); WebGPU q4: 20.8 s/page; 5.2 GB in WebKit processes | Not recommended. It only works with WebGPU, so there is no CPU fallback with these files: fp16 needs about 2.8 GB of wasm32 heap and was not tried, and fp32 does not fit in wasm32. WebKitGTK on Linux was not checked; WebView2 (Chromium) has WebGPU. Memory is 2–4× native. **By default, Transformers.js downloads the ORT wasm from `cdn.jsdelivr.net` and models from `huggingface.co`.** Both must be overridden (`env.allowRemoteModels = false`, local `env.backends.onnx.wasm.wasmPaths`) and enforced by CSP. The test did this, and only local files were requested. Detection would also move into the UI process, away from the pipeline in Rust. |
| **(c) QVAC** | `@qvac/onnx` 0.15.1 (Apache-2.0, Tether): darwin-arm64 addon **20.7 MB** (7.4 MB gzip -9); ORT API 24; CPU, XNNPACK and CoreML EPs; every desktop and mobile platform prebuilt (214 MB npm unpacked for all) | Loaded in **Bare 1.34.1, the sidecar's runtime**: `createSession` + `run` on q4 and int8; argmax identical to Python ORT on all 11,271 tokens; 0.19 s/page; 1.28 GB RSS (q4) | **Not "llama.cpp only".** QVAC's LLM, embedding and OCR addons are GGML/llama.cpp-based and cannot run this encoder. QVAC does, however, publish a generic ONNX Runtime addon that runs it inside the **existing** sidecar process. It is not part of `@qvac/inference` 0.21's pinned set, so it would be one more pinned addon, and it is still a second inference engine, but with no new process, no new IPC and the same model-import flow. Tokenisation and Viterbi would live in the worker (JS) or in Rust (sending token ids). Of the three options, this one fits the architecture best. |

## Recommendation

**Don't adopt Privacy Filter now as a new Layer 3 option or as a replacement.** Record it as the leading candidate for a fast complement, and prototype it through `@qvac/onnx` (option c) only if CPU-only speed or name recall becomes the priority.

- **It is not a replacement for Qwen3-1.7B.** Combined F1 is equal (89.2 vs 88.8), and Privacy Filter finds more: +3.2 recall, +3.3 covered recall, +5.0 held-out recall. But:
  - it cannot see company names, money or case numbers;
  - it labels every identifier as an account number;
  - it is weaker in Portuguese (−3.5 F1);
  - it gives the reviewer about 3× as many bad suggestions;
  - its model card calls it "primarily English" and flags legal workflows as high-risk.
  
  Qwen3-1.7B's prompt also covers Qvault's full category list and can be retuned. Privacy Filter's label policy can only be changed by fine-tuning.
- **As a complement, the numbers support it.** Layers 1+2 + Qwen3-1.7B + Privacy Filter would miss 7 of 461 labels instead of 30, and Privacy Filter's cost is about 0.2 s per page on CPU. That is a clear reason for a second engine *if* the product wants maximum recall, or must also run well on machines without a GPU, where Qwen3-1.7B is slow.
- **If it is prototyped:**
  - Use **int8 `model_quantized.onnx`** (1.62 GB). It is equal to the full-precision reference and the fastest on CPU; q4 loses 6.7 points of held-out recall.
  - Use opf's Viterbi decoding with the `default` point, or `stay2`/`stay3` for recall (the user-visible "more recall" setting).
  - Import it as a local file with its SHA-256 pinned, like the catalog models.
  - Give its suggestions a low confidence for `account_number` (wrong type) and for single given names.
  - Keep Layer 2: dropping it raises F1 but loses 5 points of recall.
- **What it would take beyond this measurement:** an Apache-2.0 notice; a decision on where tokenisation lives; a per-platform check of `@qvac/onnx` (only macOS arm64 was run here); and a larger, ideally real-document, Portuguese and Spanish check, because the eval set is small (461 labels, 60 held out) and synthetic.

## Reproduce

- Scripts and downloads were kept out of the repository, in the session scratchpad: model files, venv, `run_pf.py`, `ortsize/`, `qvac-onnx-test/` and `webtest/`.
- The prediction JSON format and the scoring entry point are `src-tauri/tests/eval_support/external.rs`.
- `run_pf.py` writes one JSON per operating point. `QVAULT_EVAL_EXTERNAL` scores them; the summary numbers above come from those reports.
