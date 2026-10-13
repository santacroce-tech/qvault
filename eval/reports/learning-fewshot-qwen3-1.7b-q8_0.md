# Eval: few-shot examples from review decisions, Layer 3 `qwen3-1.7b-q8_0`

16 documents, 18 pages. Examples for each document come only from the simulated review of the *other* documents (baseline Layers 1+2+3 detections: approved if they overlap a required label, rejected if they overlap none). Prompt `p3`, temp 0, seed 42. s/page = Layer 3 model time.

| variant | L1+2+3 P | R | F1 | covered R | holdout R | holdout F1 | L3 additions P / R | s/page | failed chunks | examples (mean) | prompt chars added (mean) |
|---|---:|---:|---:|---:|---:|---:|---|---:|---:|---:|---:|
| no examples (app default) | 84.9 | 93.1 | 88.8 | 85.0 | 85.0 | 87.1 | 85.2 / 10.0 | 6.21 | 0 | 0.0 | 0 |
| mixed2 | 84.6 | 93.7 | 88.9 | 85.7 | 85.0 | 85.6 | 83.1 / 10.6 | 6.24 | 0 | 2.0 | 206 |
| mixed6 | 84.9 | 92.8 | 88.7 | 84.8 | 81.7 | 84.4 | 84.9 / 9.8 | 6.09 | 0 | 6.0 | 338 |
| rejected3 | 84.7 | 94.4 | 89.3 | 86.3 | 85.0 | 86.3 | 83.9 / 11.3 | 6.25 | 0 | 3.0 | 224 |
| rejected6 | 84.8 | 93.5 | 88.9 | 85.5 | 85.0 | 85.6 | 84.2 / 10.4 | 6.28 | 0 | 6.0 | 344 |
| placebo3 | 84.4 | 94.4 | 89.1 | 86.3 | 85.0 | 85.6 | 81.2 / 11.3 | 6.23 | 0 | 3.0 | 216 |

Examples given (first three documents; + approved, − rejected):

- `mixed2` / `en_court_complaint`: −"Effective Date" (PersonName), +"Maria Santos" (PersonName)
- `mixed2` / `en_demand_letter`: −"Effective Date" (PersonName), +"Maria Santos" (PersonName)
- `mixed2` / `en_demo_pdf`: −"Effective Date" (PersonName), +"Henrike Vollmer" (PersonName)
- `mixed6` / `en_court_complaint`: −"Effective Date" (PersonName), −"CUARTA. PLAZA DE GARAJE" (Address), −"2024/0031" (CaseNumber), +"Maria Santos" (PersonName), +"June 15, 2024" (Date), +"4.380,25 €" (MonetaryAmount)
- `mixed6` / `en_demand_letter`: −"Effective Date" (PersonName), −"CUARTA. PLAZA DE GARAJE" (Address), −"2024/0031" (CaseNumber), +"Maria Santos" (PersonName), +"June 15, 2024" (Date), +"4.380,25 €" (MonetaryAmount)
- `mixed6` / `en_demo_pdf`: −"Effective Date" (PersonName), −"CUARTA. PLAZA DE GARAJE" (Address), −"2024/0031" (CaseNumber), +"Henrike Vollmer" (PersonName), +"05/18/2024" (Date), +"4.380,25 €" (MonetaryAmount)
- `rejected3` / `en_court_complaint`: −"Effective Date" (PersonName), −"CUARTA. PLAZA DE GARAJE" (Address), −"2024/0031" (CaseNumber)
- `rejected3` / `en_demand_letter`: −"Effective Date" (PersonName), −"CUARTA. PLAZA DE GARAJE" (Address), −"2024/0031" (CaseNumber)
- `rejected3` / `en_demo_pdf`: −"Effective Date" (PersonName), −"CUARTA. PLAZA DE GARAJE" (Address), −"2024/0031" (CaseNumber)
- `rejected6` / `en_court_complaint`: −"Effective Date" (PersonName), −"CUARTA. PLAZA DE GARAJE" (Address), −"2024/0031" (CaseNumber), −"Delaware Limited" (CompanyName), −"Judgment Debtor" (PersonName), −"SÃO PAULO" (Address)
- `rejected6` / `en_demand_letter`: −"Effective Date" (PersonName), −"CUARTA. PLAZA DE GARAJE" (Address), −"2024/0031" (CaseNumber), −"Delaware Limited" (CompanyName), −"Judgment Debtor" (PersonName), −"Contra Costa County" (Address)
- `rejected6` / `en_demo_pdf`: −"Effective Date" (PersonName), −"CUARTA. PLAZA DE GARAJE" (Address), −"2024/0031" (CaseNumber), −"Delaware Limited" (CompanyName), −"Judgment Debtor" (PersonName), −"Contra Costa County" (Address)
- `placebo3` / `en_court_complaint`: −"Schedule B" (CompanyName), −"Governing Law" (PersonName), −"Force Majeure" (PersonName)
- `placebo3` / `en_demand_letter`: −"Schedule B" (CompanyName), −"Governing Law" (PersonName), −"Force Majeure" (PersonName)
- `placebo3` / `en_demo_pdf`: −"Schedule B" (CompanyName), −"Governing Law" (PersonName), −"Force Majeure" (PersonName)

### Decision memory and list on top of Layers 1+2+3 (baseline answers)

| scenario | preds | FP | FP marked | FP suppressed | TP marked | TP suppressed | labels lost | list size (mean) | P / R / F1 before | P / R / F1 after suppression |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| leave one document out: memory only | 521 | 76 | 9 (11.8) | 0 (0.0) | 0 | 0 | 0 | 0.0 | 84.9 / 93.1 / 88.8 | 84.9 / 93.1 / 88.8 |
| leave one document out: list = suggested candidates (rejected ≥ 3×) | 521 | 76 | 9 (11.8) | 0 (0.0) | 0 | 0 | 0 | 1.8 | 84.9 / 93.1 / 88.8 | 84.9 / 93.1 / 88.8 |
| leave one document out: list = every rejected string | 521 | 76 | 0 (0.0) | 9 (11.8) | 0 | 0 | 0 | 64.0 | 84.9 / 93.1 / 88.8 | 86.4 / 93.1 / 89.6 |
| stress: list = every string rejected in any document, this one included | 521 | 76 | 0 (0.0) | 75 (98.7) | 0 | 0 | 0 | 68.0 | 84.9 / 93.1 / 88.8 | 99.8 / 93.1 / 96.3 |
| dev history → holdout documents: memory only | 56 | 6 | 0 (0.0) | 0 (0.0) | 0 | 0 | 0 | 0.0 | 89.3 / 85.0 / 87.1 | 89.3 / 85.0 / 87.1 |
| dev history → holdout documents: list = every rejected string | 56 | 6 | 0 (0.0) | 0 (0.0) | 0 | 0 | 0 | 62.0 | 89.3 / 85.0 / 87.1 | 89.3 / 85.0 / 87.1 |
