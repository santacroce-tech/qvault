# Roadmap

What is done, what is next, and what is parked, with the reason for each.
Engineering detail for finished work is in
[docs/qvac-integration.md](docs/qvac-integration.md); measurements are in
[eval/](eval/README.md).

Last updated: 2026-10-13.

## Done

| Phase | What | Notes |
|---|---|---|
| 0 | Design and audit | Found that PDF and DOCX export leaked redacted text; fixed in Phase 1. |
| 1 | QVAC sidecar, Model Manager, verified export | Bare sidecar over stdio, no sockets. PDF pages with redactions are rasterised; every export is re-read and refused if text survives. |
| 2 | OCR for scanned PDFs and images | QVAC OCR (CRAFT + Latin recognizer), import-only models from the `ocr-models-v1` release. |
| 3 | Eval harness, Layer 1 fixes, Layer 3 quality | Qwen3-1.7B is the default. Layers 1–2 eval runs in CI against a baseline. |
| 4 | Entity groups and known-entity suggestions | Rules first; the optional embedding model (Nomic Embed Text v2) did not change results on the eval set. |

## In progress

- **Learn from review decisions** (replaces the LoRA fine-tuning of Phase 5):
  findings rejected before are marked and can be rejected in one click; a
  user-managed "Not personal data" list that a firm can export and share;
  few-shot examples from past decisions in the Layer 3 prompt, shipped only
  if the eval shows it helps.

## Next

Ordered by expected value.

1. **Layer 2 name precision.** The name heuristic is about 52 % precise and
   caps combined precision at about 85 %; with a strong model, Layer 3 alone
   beats all three layers together. Tighten the rules, or let Layer 3 confirm
   Layer 2 names when a model is installed.
2. **A bigger, independent eval set.** The current 16 documents (461 labels)
   and the entity set (59 entities) were written by us and partly used for
   tuning. Add held-out documents written by someone else, and real-world
   layouts (letterheads, tables, signatures).
3. **Check the new screens in the real app.** The entity group screens
   (merge, split, proposals) were checked only in the browser preview with
   mock data.
4. **Benchmarks beyond Apple Silicon.** Layer 3, OCR and embedding speed and
   memory were measured only on an M4 Pro with Metal. Measure CPU-only,
   Windows (Vulkan) and Linux, and adjust the model notes and memory estimates.
5. **Model Manager notes in Spanish and Portuguese.** The per-model notes come
   from the Rust catalog and are English only.
6. **Native review of Spanish and Portuguese legal wording** in the app and on
   the website.

## Backlog

### Detection

- Scanned pages: the new address and phone patterns break on OCR line wraps
  (scan finds 92 % of text-layer detections, down from 98 %). Join wrapped
  lines before matching.
- Nicknames and middle names in entity groups ("Bob" / "Robert").
- Two people written identically ("J. Santos") share one entity group.
- Grouping proposals are about 68 % precise; ambiguous ones (a short form that
  fits two people) are only 55 % precise.
- More jurisdictions for Layer 1 (UK, FR, ES, PT, MX, AR…).
- Gemma 4 E2B / E4B scored best in the Layer 3 eval and are Apache-2.0 (not
  the older Gemma terms). Consider adding them to the catalog as larger
  options.
- Qwen2.5-7B: Qwen's official GGUF is split into two files, which the model
  store does not support. Add multi-file models if a larger tier is wanted.

### Product

- Encryption at rest. Removed on purpose (commit 3f8e0f2); documents, page
  text and findings are stored unencrypted. Revisit with OS keychain-backed
  keys if firms ask for it. Until then, docs recommend full-disk encryption.
- Code signing for macOS and Windows installers.
- New screenshots (the site shows an older version without the Model Manager
  and entity groups) and a real redacted demo PDF exported by Qvault.
- Semantic search across documents ("documents mentioning this client" works
  today only from exact occurrences).

### Website and repository

- Decide whether to retire qvault.tech (`landing/` + `deploy-website.yml`) or
  point it at the GitHub Pages site.
- Set the repository's About homepage link.
- User docs for entity groups and decision memory on the website (the Model
  Manager, OCR and offline setup are already covered on the Models page).

## Parked

- **Local LoRA fine-tuning (original Phase 5).** Built and measured, then
  stopped. Training works on the pinned QVAC version, but on a simulated
  review history every adapter made held-out results worse: Layer 3 F1 fell
  from 76.8 to 49.1 for Qwen3-1.7B and from 63.0 to 62.9 (or 16.7 with a
  higher learning rate) for Qwen3-0.6B, and training took 14–36 minutes. A
  single user's review history is too small to train on. The code and reports
  are on the local branch `experiment/phase5-lora`. Revisit only with much
  larger, firm-wide datasets.
- **Audio redaction (original Phase 6, optional).** Transcription with
  speaker diarization, review in a transcript view, redacted transcript and
  bleeped audio. Not started: it would add a speech model and an audio
  library to the installer. Revisit if firms ask for it.
- **Mobile AI and OCR.** QVAC on mobile needs Expo; Qvault's mobile apps use
  Tauri. Mobile keeps Layers 1–2 only.
