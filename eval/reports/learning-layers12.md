# Eval: learning from review decisions, Layers 1–2

Simulated reviewer: detections overlapping a required label approved, detections overlapping no label rejected (optional-only: no decision). Each document is scanned with the memory of the *other* documents. Marked = shown, pending, lower confidence. Suppressed = set aside by the "Not personal data" list (counted and restorable in the app). *Labels lost* = required labels some detection covered before suppression and none after.

### Layers 1–2

| scenario | preds | FP | FP marked | FP suppressed | TP marked | TP suppressed | labels lost | list size (mean) | P / R / F1 before | P / R / F1 after suppression |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| leave one document out: memory only | 462 | 68 | 9 (13.2) | 0 (0.0) | 0 | 0 | 0 | 0.0 | 84.9 / 83.1 / 84.0 | 84.9 / 83.1 / 84.0 |
| leave one document out: list = suggested candidates (rejected ≥ 3×) | 462 | 68 | 9 (13.2) | 0 (0.0) | 0 | 0 | 0 | 1.8 | 84.9 / 83.1 / 84.0 | 84.9 / 83.1 / 84.0 |
| leave one document out: list = every rejected string | 462 | 68 | 0 (0.0) | 9 (13.2) | 0 | 0 | 0 | 57.4 | 84.9 / 83.1 / 84.0 | 86.6 / 83.1 / 84.8 |
| stress: list = every string rejected in any document, this one included | 462 | 68 | 0 (0.0) | 68 (100.0) | 0 | 0 | 0 | 61.0 | 84.9 / 83.1 / 84.0 | 100.0 / 83.1 / 90.8 |
| dev history → holdout documents: memory only | 45 | 6 | 0 (0.0) | 0 (0.0) | 0 | 0 | 0 | 0.0 | 86.7 / 66.7 / 75.4 | 86.7 / 66.7 / 75.4 |
| dev history → holdout documents: list = every rejected string | 45 | 6 | 0 (0.0) | 0 (0.0) | 0 | 0 | 0 | 55.0 | 86.7 / 66.7 / 75.4 | 86.7 / 66.7 / 75.4 |

### What the memory marks (leave one document out)

| category | text (normalised) | correct detection? | times marked |
|---|---|---|---:|
| PersonName | effective date | no | 3 |
| PersonName | confidential information | no | 2 |
| PersonName | managing director | no | 2 |
| PersonName | sao paulo | no | 2 |
