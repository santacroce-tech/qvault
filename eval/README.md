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
