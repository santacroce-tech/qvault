# Qvault detection eval

Measures how well Qvault's three PII detection layers find what a lawyer
would redact, on synthetic legal documents in English, Spanish and
Portuguese. Results and the decisions they support are in
[docs/qvac-integration.md §11](../docs/qvac-integration.md#11-phase-3-layer-3-quality-model-choice-and-the-eval-harness).

Everything here is synthetic. Names, numbers and addresses are invented and
format-valid where that matters (reserved 555-01xx phones, documentation
IBANs, test card numbers, `example.*`/`.test` domains). **Never add real
personal data**: this repository is public.

## Layout

| Path | What |
|---|---|
| `fixtures/src/*.txt` | Annotated sources (edit these) |
| `fixtures/*.json` | Generated fixtures with byte offsets (do not edit by hand) |
| `build-fixtures.mjs` | Builds and validates the JSON from the sources |
| `baseline.json` | Committed Layers 1–2 numbers that CI must not fall below |
| `reports/` | Reports referenced by the docs (Layers 1–2 and Layer 3 model runs) |
| `../src-tauri/tests/eval_harness.rs` | Scoring, the CI test, the report writer |
| `../src-tauri/tests/eval_support/layer3.rs` | Opt-in Layer 3 runs with real models |
| `../src-tauri/tests/eval_support/external.rs` | Opt-in scoring of predictions made outside Qvault (`QVAULT_EVAL_EXTERNAL`) |
| `../src-tauri/tests/eval_support/privacy_filter.rs` | Opt-in Privacy Filter Layer 3 through the app's pipeline (`QVAULT_EVAL_PF`) |
| `../src-tauri/tests/privacy_filter_it.rs` | Opt-in Privacy Filter golden tests (tokenizer, decoder vs opf, import, download) |

## Label format

A source file has a header, a `---` line, then the document text with inline
labels. Pages are separated by a line `=== page ===`.

```
id: en_court_complaint
lang: en                 # en | es | pt
kind: court filing
split: dev               # dev (default) | holdout
description: what the fixture tests
---
Plaintiff [[PersonName|Dana Holloway]] lives at [[Address|2287 Juniper Hollow Road, Oakland, CA 94611]].
The bank was [[?CompanyName|Canyon State Bank]].
```

- `[[Category|text]]`: a required label. `Category` is one of Qvault's 23
  `PiiCategory` names (`PersonName`, `Email`, …, `CompanyName`).
- `[[?Category|text]]`: optional. Missing it is not an error, and flagging it
  is not a false positive (used for ambiguous items such as a bank named only
  as an institution, a public judge in a citation, a registry number).
- Labelling guide: every calendar date is `Date` (birth dates are
  `DateOfBirth`); money amounts are labelled; routing numbers and SWIFT/BIC
  codes are `BankAccount`; companies are labelled when they are parties,
  employers or counterparties; courts, agencies, laws, statutes, case-law
  citations, clause headings and defined terms are not labelled (they are
  the false-positive traps). Titles such as "Dr." and "Mr." are outside the
  name span.

`node eval/build-fixtures.mjs` writes `fixtures/<id>.json`:

```json
{"id": "...", "lang": "en", "split": "dev", "kind": "...", "description": "...", "source": "src/....txt",
 "pages": [{"text": "...", "spans": [{"start": 10, "end": 23, "category": "PersonName", "text": "Dana Holloway"}]}]}
```

`start`/`end` are **UTF-8 byte offsets** into `text` (what the Rust pipeline
reports). Both the builder and the Rust harness validate every span: it must
reproduce its `text` exactly, sit on character boundaries, use a known
category, and not overlap the previous span. `--check` fails if a JSON file is
stale or invalid (CI runs it).

The `holdout` fixtures (`holdout_*.txt`) were written after the Layer 1
regex work and were never used to tune any rule or prompt. Keep it that way:
when you change a detector, look at the dev errors only, and report holdout
numbers separately.

## Scoring

Matching ignores the category (a name redacted as a company is still
redacted); category agreement is reported as *type accuracy*.

- **overlap** (headline): a prediction matches a label if they share at least
  one byte. Recall = labels matched / labels; precision = predictions that
  match a label / predictions.
- **exact**: same boundaries, ignoring surrounding whitespace and trailing
  `.,;:`.
- **covered** (recall only): every letter and digit of the label lies inside
  some prediction. This is the redaction view: an overlap that leaves "Santos"
  visible in "Maria Santos" is not covered.
- Predictions that touch only optional labels are ignored for precision.

Reports list every false negative (`missed` = nothing overlaps, `partial` =
overlap but not covered) and every false positive with its layer.

## Running

All commands from `src-tauri/`.

**Layers 1–2 (deterministic, no models, runs in CI):**

```sh
cargo test --test eval_harness -- --nocapture
# report: target/eval/layers12.{md,json}
QVAULT_EVAL_OUT=../eval/reports cargo test --test eval_harness layers_1_2   # write into eval/reports
QVAULT_EVAL_UPDATE_BASELINE=1 cargo test --test eval_harness layers_1_2     # accept new numbers
```

The test fails when overall overlap/exact/covered recall, or the overlap recall
of any category or language, drops by more than 0.5 points below
`baseline.json`, or overall precision by more than 2 points. If a change is
intended, update the baseline in the same commit and say why.

**Layer 3 with real models (opt-in):** needs the sidecar (`npm run
sidecar:build` at the repo root) and model files.

```sh
QVAULT_EVAL_L3=1 \
QVAC_MODELS_DIR=/path/with/catalog/files \
QVAULT_EVAL_MODELS=qwen2.5-0.5b-instruct-q4_k_m,qwen3-4b=/abs/path/Qwen3-4B-Q4_K_M.gguf \
QVAULT_EVAL_RUNS=3 \
QVAULT_EVAL_PROMPTS=p3,p1b \
QVAULT_EVAL_OUT=../eval/reports \
cargo test --release --test eval_harness layer3 -- --nocapture --test-threads 1
```

- `QVAULT_EVAL_MODELS`: catalog ids (looked up in `QVAC_MODELS_DIR` by their
  catalog file name) or `name=/abs/path.gguf` for any other GGUF, which is
  hashed and installed into a throw-away models dir as a custom model.
- `QVAULT_EVAL_RUNS` (default 3): repeated runs with the first prompt, for
  run-to-run variance.
- `QVAULT_EVAL_PROMPTS` (default `p3`): prompt versions; each extra one gets a
  single run. `p1b` is the Phase 1B prompt.
- `QVAULT_EVAL_CHUNK_WORDS`: chunk size override (overlap = size / 7, min 8).
- `QVAULT_EVAL_SUMMARY`: file name of the per-model summary (default
  `layer3-models`).
- `QVAULT_EVAL_FIXTURES`: comma-separated fixture ids to run (quick checks).
- `QVAULT_EVAL_VERBOSE=1`: print the time and answer size of every chunk.

Keep the machine awake (`caffeinate -ims …` on macOS): if it sleeps, a run
stalls until it wakes. Seconds per page use a monotonic clock and are not
affected, but wall time is. Layer 3 runs with temp 0 and a fixed seed and
was byte-identical across runs on the dev machine, so one run per model is
enough for comparisons once determinism has been checked.

Per model it writes `layer3-<model>.{md,json}`: assessFit verdict, load time,
seconds per page (model time only), peak RSS of the sidecar process tree,
precision/recall/F1 for Layer 3 alone and Layers 1+2+3 (overall, per
language, dev/holdout), mean ± sd over runs, run-to-run span Jaccard and the
share of byte-identical answers, a comparison of prompt versions, an
**ablation** of every Layer 3 post-processing heuristic (replayed on the
recorded answers of run 1, so the model is not re-run), what happened to the
model's items (invalid, implausible, stop-listed, not in the chunk, …), and
per-category precision used to calibrate confidence.

Peak RSS is sampled with `ps` every 300 ms. On Apple Silicon, Metal buffers
backed by the memory-mapped model file count towards RSS; GPU-private
allocations may not.

**External predictions (opt-in):** scores spans produced outside Qvault, for
example another model run in Python, as if they came from Layer 3. They are
scored alone, added to Layers 1+2 (a span that overlaps a Layer 1–2
detection is dropped, as in the app), and added to Layer 1 alone.

```sh
QVAULT_EVAL_EXTERNAL=/abs/a.json,/abs/b.json QVAULT_EVAL_OUT=../eval/reports \
cargo test --test eval_harness external_predictions -- --nocapture
# report per file: external-<system>.{md,json}
```

File format: `{"system": "...", "meta": {...}, "fixtures": {"<fixture id>": [[{"start", "end", "category", "text"}, ...], ...]}}`,
with one inner list per page. `start`/`end` are UTF-8 byte offsets into the
fixture page text, and `category` is a Qvault category name. Every span is
validated (range, char boundary, `text` equal to the slice, known category),
and the test fails on any mismatch. Used for OpenAI Privacy Filter:
[`reports/privacy-filter.md`](reports/privacy-filter.md).

## Privacy Filter as Layer 3 (§15)

Results and decisions: [docs/qvac-integration.md §15](../docs/qvac-integration.md#15-openai-privacy-filter-as-the-default-layer-3-hybrid-with-the-language-model).
Report: [`reports/privacy-filter-layer3.md`](reports/privacy-filter-layer3.md) (and `.json`).

OpenAI Privacy Filter runs through the same code the app uses:
- the QVAC sidecar (`@qvac/onnx`) via `QvacService`;
- Qvault's tokenizer, decoder and scanner.

Every page is classified once; its logits are kept and timed, along with the sidecar's peak RSS. Every operating point (Viterbi transition biases) and post-processing filter is then replayed on the recorded logits, so the variants differ only in what they change.

**Selection.** Variants are compared on the **dev** split with a rule fixed in the code:
1. keep the variants within 0.5 points of the best dev recall;
2. among those, take the fewest dev false suggestions;
3. break ties by dev F1.

Variants that drop short single-word names are listed but are not eligible. The holdout split is only reported. The report shows the rule's pick next to the app default (`PfConfig::default()`). See §15.6 for why they differ.

**False suggestions** are detections a Layer 3 detector adds to Layers 1–2 that overlap no label: what the reviewer has to reject.

With `QVAULT_EVAL_MODELS`, a catalog language model also runs once (answers recorded). That adds the rows "Layers 1+2 + LLM" and "Layers 1+2 + Privacy Filter + LLM", merged the way the app's thorough mode merges them (longer span wins).

```sh
# QVAULT_PF_MODEL_DIR: the four model files, flat or in the Hugging Face layout (onnx/ subfolder)
QVAULT_EVAL_PF=1 QVAULT_PF_MODEL_DIR=/path/to/privacy-filter \
QVAC_MODELS_DIR=/path/with/catalog/files QVAULT_EVAL_MODELS=qwen3-1.7b-q8_0 \
QVAULT_EVAL_OUT=../eval/reports \
cargo test --release --test eval_harness privacy_filter -- --nocapture --test-threads 1
```

Without the language model, a run takes about 6 s; with Qwen3-1.7B, about 2 minutes on Apple M4 Pro. It needs the sidecar (`npm run sidecar:build`). Nothing here runs in CI: it depends on the model files. The CI Layers 1–2 baseline is unchanged.

Golden tests (opt-in, same model directory):

```sh
QVAULT_PF_MODEL_DIR=/path/to/privacy-filter \
cargo test --release --test privacy_filter_it -- --nocapture --test-threads 1
# + QVAULT_PF_DOWNLOAD_IT=1 to download the pinned files from Hugging Face (1.65 GB, with a cancel test)
# + QVAULT_PF_DUMP=/dir to write each page's token ids and logits (for checking against OpenAI's opf)
```

- `tests/fixtures/privacy-filter/o200k_ids.json`: tiktoken `o200k_base` ids of every eval page and 34 edge cases.
- `reference_spans_qvac_onnx.json`: opf's decoding of the logits of the engine Qvault ships (`@qvac/onnx` 0.15.1). This is the strict golden.
- `reference_spans.json`: opf on Python onnxruntime 1.31 logits (the §14 eval). Agreement with it is reported and must stay at least 97 %.

Regenerate the strict golden after an `@qvac/onnx` upgrade: dump the logits with `QVAULT_PF_DUMP`, then decode them with opf (§15.3).

## Entity resolution and known-entity suggestions (Phase 4)

Results and decisions: [docs/qvac-integration.md §12](../docs/qvac-integration.md#12-phase-4-entity-resolution).
Code: `../src-tauri/tests/eval_support/entities.rs` (part of the
`eval_harness` test binary).

| Path | What |
|---|---|
| `entities/variants.json` | Synthetic entity-resolution set: 59 EN/ES/PT entities, 199 mentions |
| `entity-baseline.json` | No-model numbers CI must not fall below |
| `reports/entity-resolution*.md` | Grouping without / with embedding models |
| `reports/known-entity-*.md` | Known-entity suggestions on the detection fixtures |

`variants.json` lists, per entity, the ways it is written (`"mentions"`).
Mentions of *different* entities of one category are the hard negatives:
same surname and different given name, same given name, Jr. / Filho, the
same company with another legal form, one-letter-different companies,
neighbouring house numbers. A string may appear under two entities on
purpose ("J. Santos"): the app keeps one variant per distinct string, so no
system can separate them, and the score shows it.

Grouping is scored on mention pairs of the same category (pairwise P/R/F1)
and with B-cubed, three ways: **applied automatically** (what the app does
on its own), **after review** (a reviewer accepts the correct proposals and
dismisses the rest) and **if every proposal is accepted**; plus proposal
precision. Cross-category pairs are never linked (asserted).

Known-entity suggestions are run on the detection fixtures with three
knowledge bases: the document's own detections before review (what the app
does right after a scan), the document's correct detections (as if
reviewed), and every labelled entity of the document approved in an earlier
document (upper bound). *Recovered* = labels no layer overlapped that a
suggestion overlaps; *false* = suggestions that overlap no label.

```sh
# CI (no model): grouping + suggestions on Layers 1-2 vs entity-baseline.json
cargo test --test eval_harness entities:: -- --nocapture
QVAULT_EVAL_UPDATE_BASELINE=1 cargo test --test eval_harness entities::   # accept new numbers

# Embedding models through the real sidecar (sweep of thresholds, speed, memory)
QVAULT_EVAL_EMBED=1 QVAC_MODELS_DIR=/path/with/catalog/files \
QVAULT_EVAL_EMBED_MODELS=nomic-embed-text-v2-moe-q8_0,qwen3-embedding-0.6b-q8_0=/abs/Qwen3-Embedding-0.6B-Q8_0.gguf \
QVAULT_EVAL_OUT=../eval/reports \
cargo test --release --test eval_harness entity_resolution_embeddings -- --nocapture --test-threads 1

# Suggestions on top of Layers 1-3 with a real LLM
QVAULT_EVAL_L3=1 QVAULT_EVAL_KNOWN=1 QVAC_MODELS_DIR=/path QVAULT_EVAL_MODELS=qwen3-1.7b-q8_0 \
QVAULT_EVAL_OUT=../eval/reports \
cargo test --release --test eval_harness known_entity_suggestions_layer3 -- --nocapture --test-threads 1
```

The CI test fails when applied / after-review / accepted precision or
recall or proposal precision drops by more than 1 point below
`entity-baseline.json`, when a knowledge-base scenario recovers fewer
labels, or when it adds more than one extra false suggestion.

## Learning from review decisions (§13)

Results and decisions: [docs/qvac-integration.md §13](../docs/qvac-integration.md#13-learning-from-review-decisions-replaces-phase-5-lora).
Code: `../src-tauri/tests/eval_support/learning.rs` (part of the
`eval_harness` test binary).

| Path | What |
|---|---|
| `reports/learning-layers12.{md,json}` | Decision memory and the "Not personal data" list on Layers 1–2 |
| `reports/learning-fewshot-<model>.{md,json}` | Few-shot examples in the Layer 3 prompt, plus memory and list on Layers 1–3 |

There is no labelled review history, so it is simulated from the detection
fixtures. A detection that overlaps a required label is *approved*. One that
overlaps no label is *rejected*. One that touches only optional labels gets
no decision.

The history is always split **by document**: each fixture is scanned with
the memory of the *other* fixtures (leave one out, holdout fixtures
included). A second split uses the history of the dev fixtures to scan the
holdout fixtures.

Measured:

- **memory**: false positives marked "rejected before", and correct
  detections marked by mistake;
- **list**: false positives suppressed, correct detections suppressed, and
  *labels lost* (required labels covered before suppression and not after).
  The list is filled three ways: suggested candidates (rejected ≥ 3 times),
  every rejected string, and a stress case with every string rejected
  anywhere, the scanned document included;
- **few-shot**: Layers 1+2+3 precision, recall, F1, covered recall and the
  holdout split, seconds per page and prompt length, with and without
  examples. Each document gets examples only from the other documents.
  Variants are `mixed<N>` (half rejected, half approved), `rejected<N>`, and
  `placebo<N>`, a control with generic terms that no review produced. The
  control separates "the user's decisions help" from "any prompt change
  moves the numbers".

```sh
# CI (no model): memory and list on Layers 1-2. Fails if the list ever
# removes a labelled item, in any scenario.
cargo test --test eval_harness learning::decision_memory_layers12 -- --nocapture
QVAULT_EVAL_OUT=../eval/reports cargo test --test eval_harness learning::decision_memory_layers12

# Few-shot Layer 3 with a real model (about 2 minutes per variant for Qwen3-1.7B on Apple M4 Pro)
QVAULT_EVAL_FEWSHOT=1 QVAC_MODELS_DIR=/path/with/catalog/files QVAULT_EVAL_MODELS=qwen3-1.7b-q8_0 \
QVAULT_EVAL_FEWSHOT_VARIANTS=mixed2,mixed6,rejected3,rejected6,placebo3 \
QVAULT_EVAL_OUT=../eval/reports \
cargo test --release --test eval_harness fewshot_layer3 -- --nocapture --test-threads 1
```
