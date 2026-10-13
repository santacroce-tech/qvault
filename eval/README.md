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
