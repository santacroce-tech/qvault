# Eval: Layer 3 with `qwen2.5-0.5b-instruct-q4_k_m`

assessFit: **likely-fits** · load 0.8 s · 18 pages · 3 run(s) · prompt `p3` · chunks of 150 words, 20 overlap · post-processing: default

## Overall (overlap matching; mean ± sd over runs)

| | P | R | F1 | covered R | EN F1 | ES F1 | PT F1 |
|---|---|---|---|---|---|---|---|
| Layer 3 alone | 89.7 ± 0.0 | 37.7 ± 0.0 | 53.1 ± 0.0 | 34.5 ± 0.0 | 51.8 ± 0.0 | 54.8 ± 0.0 | 55.0 ± 0.0 |
| Layers 1+2 | 84.9 | 83.1 | 84.0 | 75.5 | 80.0 | 88.2 | 90.2 |
| Layers 1+2+3 | 82.3 ± 0.0 | 87.0 ± 0.0 | 84.6 ± 0.0 | 79.2 ± 0.0 | 80.7 ± 0.0 | 89.0 ± 0.0 | 90.2 ± 0.0 |

Speed: 4.41 ± 0.32 s/page (Layer 3 only; 53 chunks/run, 0 failed chunks over all runs: 0 engine errors, 0 unparseable answers). Peak RSS of the sidecar: 942 MB.

Run-to-run: span-set Jaccard 100.0 ± 0.0 (pairs of runs), identical raw answers 100.0 ± 0.0 of chunks.

### Exact / overlap / covered, run 1

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| Layer 3 alone | 461 | 194 | 89.7 / 37.7 / 53.1 | 76.3 / 32.1 / 45.2 | 34.5 | 62.6 |
| Layers 1+2+3 | 461 | 485 | 82.3 / 87.0 / 84.6 | 73.4 / 77.2 / 75.3 | 79.2 | 95.5 |
| Layers 1+2+3 – dev | 401 | 434 | 81.6 / 88.5 / 84.9 | 74.0 / 80.0 / 76.9 | 82.0 | 96.3 |
| Layers 1+2+3 – holdout | 60 | 51 | 88.2 / 76.7 / 82.0 | 68.6 / 58.3 / 63.1 | 60.0 | 89.1 |

### Prompt versions (run 1 each)

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| `p3` Layer 3 alone (run 1) | 461 | 194 | 89.7 / 37.7 / 53.1 | 76.3 / 32.1 / 45.2 | 34.5 | 62.6 |
| `p3` Layers 1+2+3 (run 1) | 461 | 485 | 82.3 / 87.0 / 84.6 | 73.4 / 77.2 / 75.3 | 79.2 | 95.5 |
| Phase 1B (p1b prompt + p1b post) Layer 3 alone | 461 | 159 | 83.6 / 28.2 / 42.2 | 56.0 / 19.3 / 28.7 | 22.3 | 48.5 |
| Phase 1B (p1b prompt + p1b post) Layers 1+2+3 | 461 | 479 | 80.6 / 83.9 / 82.2 | 71.6 / 74.4 / 73.0 | 76.4 | 96.9 |
| `p1b` Layer 3 alone (run 1) | 461 | 169 | 85.2 / 30.4 / 44.8 | 55.6 / 20.4 / 29.8 | 23.4 | 46.4 |
| `p1b` Layers 1+2+3 (run 1) | 461 | 479 | 81.0 / 84.4 / 82.7 | 71.8 / 74.6 / 73.2 | 76.6 | 96.9 |

### Post-processing ablation (replayed on run 1's answers)

| config | L3 P | L3 R | L3 F1 | L1+2+3 P | L1+2+3 R | L1+2+3 F1 |
|---|---|---|---|---|---|---|
| default | 89.7 | 37.7 | 53.1 | 82.3 | 87.0 | 84.6 |
| Phase 1B post-processing | 86.0 | 33.4 | 48.1 | 81.2 | 85.9 | 83.5 |
| − stop-list | 83.7 | 37.7 | 52.0 | 80.0 | 87.0 | 83.3 |
| − Last, First variants | 89.6 | 37.5 | 52.9 | 82.2 | 86.8 | 84.4 |
| − honorific + surname | 89.6 | 37.5 | 52.9 | 82.2 | 86.8 | 84.4 |
| − page-wide mapping | 89.1 | 33.8 | 49.1 | 82.3 | 86.3 | 84.3 |
| − accent folding | 89.7 | 37.7 | 53.1 | 82.3 | 87.0 | 84.6 |
| − hyphen line-wrap | 89.7 | 37.7 | 53.1 | 82.3 | 87.0 | 84.6 |

### What happened to the model's items (run 1)

```
{
  "items": 863,
  "salvaged_chunks": 18,
  "invalid_items": 0,
  "unknown_category": 0,
  "empty_text": 0,
  "implausible": 45,
  "stoplisted": 3,
  "not_in_chunk": 107,
  "accepted": 213,
  "variant_hits": 2,
  "page_wide_hits": 22,
  "covered_by_layers_1_2": 0
}
```

### Layer 3 alone by category (all runs pooled; for confidence calibration)

| category | preds | P (overlap) | R (overlap) | type acc. |
|---|---:|---:|---:|---:|
| Address | 186 | 80.6 | 75.9 | 95.5 |
| BankAccount | 12 | 100.0 | 33.3 | 40.0 |
| CaseNumber | 12 | 50.0 | 50.0 | 66.7 |
| ClientId | 9 | 100.0 | 40.0 | 100.0 |
| CompanyName | 60 | 95.0 | 62.2 | 78.3 |
| CourtFileNumber | 3 | 0.0 | 0.0 | 100.0 |
| CreditCard | 0 | – | 12.5 | 0.0 |
| Date | 42 | 78.6 | 41.7 | 36.7 |
| DateOfBirth | 57 | 100.0 | 22.2 | 100.0 |
| DriversLicense | 0 | – | 25.0 | 0.0 |
| Email | 21 | 100.0 | 36.0 | 77.8 |
| IBAN | 0 | – | 33.3 | 0.0 |
| IpAddress | 0 | – | 50.0 | 0.0 |
| LicensePlate | 0 | – | 0.0 | 100.0 |
| MedicalRecordNumber | 0 | – | 25.0 | 0.0 |
| MonetaryAmount | 42 | 100.0 | 30.4 | 100.0 |
| NationalId | 69 | 95.7 | 47.1 | 100.0 |
| PassportNumber | 0 | – | 33.3 | 0.0 |
| PersonName | 27 | 100.0 | 13.3 | 60.0 |
| PhoneNumber | 42 | 100.0 | 80.0 | 65.0 |
| SocialSecurityNumber | 0 | – | 66.7 | 0.0 |
| TaxId | 0 | – | 43.8 | 0.0 |
| Url | 0 | – | 66.7 | 0.0 |

Agreement (run 1): detections confirmed by two overlapping chunks: 14/15 correct (93.3); single-chunk: 160/179 (89.4).

### Layer 3 additions to Layers 1+2 by category (all runs pooled; confidence calibration)

| category | added | correct | P |
|---|---:|---:|---:|
| Address | 45 | 15 | 33.3 |
| CaseNumber | 6 | 0 | 0.0 |
| CompanyName | 12 | 9 | 75.0 |
| CourtFileNumber | 3 | 0 | 0.0 |
| Date | 9 | 0 | 0.0 |
| DateOfBirth | 9 | 9 | 100.0 |
| NationalId | 12 | 9 | 75.0 |
| PersonName | 12 | 12 | 100.0 |
| **all** | 108 | 54 | 50.0 |

### Layers 1+2+3 by category, run 1

| category | labels | L1+2 P / R | L3 P / R | L1+2+3 P / R |
|---|---:|---|---|---|
| Address | 29 | 100.0 / 89.7 | 80.6 / 75.9 | 75.6 / 96.6 |
| BankAccount | 15 | 100.0 / 73.3 | 100.0 / 33.3 | 100.0 / 80.0 |
| CaseNumber | 6 | 100.0 / 83.3 | 50.0 / 50.0 | 71.4 / 83.3 |
| ClientId | 5 | 100.0 / 80.0 | 100.0 / 40.0 | 100.0 / 80.0 |
| CompanyName | 37 | 92.0 / 81.1 | 95.0 / 62.2 | 89.7 / 89.2 |
| CourtFileNumber | 1 | 100.0 / 100.0 | 0.0 / 0.0 | 50.0 / 100.0 |
| CreditCard | 8 | 100.0 / 100.0 | – / 12.5 | 100.0 / 100.0 |
| Date | 72 | 100.0 / 91.7 | 78.6 / 41.7 | 95.7 / 95.8 |
| DateOfBirth | 9 | 100.0 / 88.9 | 100.0 / 22.2 | 100.0 / 88.9 |
| DriversLicense | 4 | 100.0 / 100.0 | – / 25.0 | 100.0 / 100.0 |
| Email | 25 | 100.0 / 96.0 | 100.0 / 36.0 | 100.0 / 96.0 |
| IBAN | 9 | 100.0 / 100.0 | – / 33.3 | 100.0 / 100.0 |
| IpAddress | 6 | 100.0 / 100.0 | – / 50.0 | 100.0 / 100.0 |
| LicensePlate | 5 | 100.0 / 100.0 | – / 0.0 | 100.0 / 100.0 |
| MedicalRecordNumber | 4 | 100.0 / 100.0 | – / 25.0 | 100.0 / 100.0 |
| MonetaryAmount | 46 | 100.0 / 100.0 | 100.0 / 30.4 | 100.0 / 100.0 |
| NationalId | 17 | 100.0 / 88.2 | 95.7 / 47.1 | 94.7 / 100.0 |
| PassportNumber | 3 | 100.0 / 100.0 | – / 33.3 | 100.0 / 100.0 |
| PersonName | 113 | 50.0 / 54.0 | 100.0 / 13.3 | 51.5 / 60.2 |
| PhoneNumber | 25 | 100.0 / 100.0 | 100.0 / 80.0 | 100.0 / 100.0 |
| SocialSecurityNumber | 3 | 100.0 / 100.0 | – / 66.7 | 100.0 / 100.0 |
| TaxId | 16 | 100.0 / 100.0 | – / 43.8 | 100.0 / 100.0 |
| Url | 3 | 100.0 / 100.0 | – / 66.7 | 100.0 / 100.0 |

### Layers 1+2+3 errors, run 1

False negatives (96):

- `en_court_complaint` p1 PersonName (missed): "Marcus T. Feeney"
- `en_court_complaint` p1 PersonName (partial): "Marcus T. Feeney"
- `en_court_complaint` p1 PersonName (missed): "Feeney"
- `en_court_complaint` p1 PersonName (missed): "Priya\nRamaswamy"
- `en_court_complaint` p1 PersonName (missed): "Feeney"
- `en_court_complaint` p1 PersonName (missed): "Ramaswamy"
- `en_court_complaint` p1 PersonName (missed): "Ellen K. Marsh"
- `en_court_complaint` p1 CompanyName (partial): "Marsh & Okonjo LLP"
- `en_demand_letter` p1 CompanyName (partial): "Okafor Whitfield PLLC"
- `en_demand_letter` p1 PersonName (missed): "Tobias R. Lindqvist"
- `en_demand_letter` p1 PersonName (partial): "Harriet Pemberton-Shaw"
- `en_demand_letter` p1 ClientId (partial): "NGM-24-0073318"
- `en_demand_letter` p1 PersonName (missed): "Tobias R. Lindqvist"
- `en_demand_letter` p1 PersonName (missed): "Lindqvist"
- `en_demand_letter` p1 PersonName (missed): "Lindqvist"
- `en_demand_letter` p1 MedicalRecordNumber (partial): "MRN 00482913"
- `en_demand_letter` p1 PersonName (missed): "A. N. Castellanos"
- `en_demand_letter` p1 PersonName (missed): "Lindqvist"
- `en_demand_letter` p1 PersonName (missed): "J. Whitfield"
- `en_demo_pdf` p1 Email (missed): "j.richardson@atlas-  \n  digital.com"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 PersonName (missed): "Felix Brandauer"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine France SAS"
- `en_eu_share_purchase` p1 CaseNumber (missed): "16 O 248/24"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 PersonName (missed): "Joost van der Linde"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 PersonName (missed): "Joost van der Linde"
- `en_garnishment_affidavit` p1 CompanyName (missed): "Desert Sun Credit Union"
- `en_garnishment_affidavit` p1 PersonName (missed): "Calloway, Brent J."
- `en_garnishment_affidavit` p1 PersonName (missed): "LORNA FETTERMAN"
- `en_garnishment_affidavit` p1 CompanyName (missed): "Desert Sun Credit Union"
- `en_garnishment_affidavit` p1 PersonName (missed): "Brent J. Calloway"
- `en_garnishment_affidavit` p1 BankAccount (partial): "0047-221983"
- `en_garnishment_affidavit` p1 BankAccount (missed): "884 120 33571"
- `en_garnishment_affidavit` p1 BankAccount (missed): "3266 0519 8820"
- `en_settlement_agreement` p1 PersonName (missed): "Siobhan Achterberg"
- `en_settlement_agreement` p1 PersonName (missed): "S. Achterberg"
- `en_settlement_agreement` p1 PersonName (missed): "Achterberg"
- `en_settlement_agreement` p1 PersonName (missed): "Rafferty\nOkonkwo-Bell"
- `en_settlement_agreement` p1 PersonName (missed): "Ms Achterberg"
- `en_settlement_agreement` p1 PersonName (missed): "Gideon Farquharson"
- `en_settlement_agreement` p1 PersonName (missed): "Achterberg, Siobhan"
- `en_settlement_agreement` p1 PersonName (missed): "Farquharson, Gideon"
- `en_settlement_agreement` p1 PersonName (missed): "SIOBHAN ACHTERBERG"
- `en_settlement_agreement` p1 PersonName (missed): "GIDEON FARQUHARSON"
- `en_traps_llc_agreement` p1 PersonName (missed): "Constance Varga-Ellison"
- `en_traps_llc_agreement` p1 PersonName (missed): "Rupert Ashdown"
- `en_traps_llc_agreement` p1 PersonName (partial): "Constance Varga-Ellison"
- `es_carta_requerimiento` p1 CompanyName (partial): "Bufete Arriaga & Solís, S.C."
- `es_carta_requerimiento` p1 PersonName (missed): "Ramírez de la Garza"
- `es_carta_requerimiento` p1 PersonName (missed): "J. M. Ortuño"
- `es_contrato_arrendamiento` p1 PersonName (partial): "Tomás Ezequiel Barroso\nQuintanilla"
- `es_contrato_arrendamiento` p1 CompanyName (partial): "Inversiones Quintanilla Hermanos, S.L."
- `es_contrato_arrendamiento` p1 PersonName (missed): "Rogelio Quintanilla Mateu"
- `es_contrato_arrendamiento` p1 PersonName (partial): "Tomás E. Barroso Quintanilla"
- `es_demanda_reclamacion` p1 PersonName (missed): "LLORENTE ESCRIVÁ, Nuria"
- `es_demanda_reclamacion` p1 CompanyName (partial): "Reformas Turia Integral, S.L.U."
- `es_demanda_reclamacion` p1 PersonName (missed): "Fermín Aleixandre Bou"
- `es_demanda_reclamacion` p1 PersonName (missed): "Aleixandre"
- `holdout_en_intake_memo` p1 PersonName (partial): "Naomi Fairbanks-Oyelaran"
- `holdout_en_intake_memo` p1 Date (missed): "2025.03.18"
- `holdout_en_intake_memo` p1 PersonName (partial): "Desmond Achebe-Kowalczyk"
- `holdout_en_intake_memo` p1 CompanyName (missed): "Pellucid Freight Systems, Inc."
- `holdout_en_intake_memo` p1 ClientId (missed): "M-25-0331"
- `holdout_en_intake_memo` p1 PersonName (partial): "Desmond Achebe-Kowalczyk"
- `holdout_en_intake_memo` p1 DateOfBirth (missed): "07.11.1968"
- `holdout_en_intake_memo` p1 Address (partial): "83 Lantern Hill Terrace, Unit 12, Marietta, GA 30064-3317"
- `holdout_en_intake_memo` p1 Address (missed): "2210 Rue Sainte-Catherine Ouest, Montréal, QC H3H 1M7"
- `holdout_en_intake_memo` p1 PersonName (missed): "Desmond"
- `holdout_en_intake_memo` p1 Date (missed): "Feb. 3, 2021"
- `holdout_en_intake_memo` p1 Date (missed): "Nov. 30, 2024"
- `holdout_en_intake_memo` p1 BankAccount (missed): "000123456789"
- `holdout_en_intake_memo` p1 CompanyName (partial): "Quist Harrow & Lyle LLP"
- `holdout_es_acta_junta` p1 CompanyName (missed): "TALLERES METÁLICOS ARGAÑA, S.L."
- `holdout_es_acta_junta` p1 PersonName (partial): "Íñigo Zabaleta Urrutia"
- `holdout_es_acta_junta` p1 PersonName (missed): "Zabaleta"
- `holdout_es_acta_junta` p1 PersonName (missed): "Beloki"
- `holdout_es_acta_junta` p1 CompanyName (partial): "Auditores Ibaizabal, S.L.P."
- `holdout_es_acta_junta` p1 PersonName (missed): "Koldo Iturbe Larrañaga"
- `holdout_es_acta_junta` p1 Address (partial): "Alameda de Recalde, 27, 5.º dcha., 48009 Bilbao"
- `holdout_es_acta_junta` p1 PhoneNumber (partial): "0034 944 555 120"
- `holdout_es_acta_junta` p1 PersonName (partial): "Íñigo Zabaleta Urrutia"
- `holdout_pt_notificacao` p1 PersonName (missed): "Vasconcelos"
- `pt_contrato_prestacao` p1 PersonName (partial): "Helena Cristina dos Santos Albuquerque"
- `pt_contrato_prestacao` p1 PersonName (partial): "Priscila\nFagundes Teixeira"
- `pt_contrato_prestacao` p1 CompanyName (partial): "LATICÍNIOS SERRA AZUL LTDA."
- `pt_contrato_prestacao` p1 PersonName (partial): "Helena Cristina dos Santos Albuquerque"
- `pt_peticao_inicial` p1 PersonName (partial): "CARVALHO, Juliana Rezende de"
- `pt_peticao_inicial` p1 PersonName (partial): "Edvaldo\nNogueira Paixão"
- `pt_peticao_inicial` p1 PersonName (missed): "Paixão"
- `pt_procuracao_portugal` p1 CompanyName (partial): "Douro Vinhos & Quintas, Lda."
- `pt_procuracao_portugal` p1 PersonName (missed): "Fonseca, Joaquim"
- `pt_procuracao_portugal` p1 PersonName (missed): "J. M. T. Fonseca"

False positives (86):

- `en_court_complaint` p1 PersonName [heuristic]: "See Burlington"
- `en_court_complaint` p1 PersonName [heuristic]: "Civil Rights Act"
- `en_demand_letter` p1 PersonName [heuristic]: "Claims Department"
- `en_demand_letter` p1 PersonName [heuristic]: "Claim No"
- `en_demand_letter` p1 PersonName [heuristic]: "Dear Claims Representative"
- `en_demand_letter` p1 PersonName [heuristic]: "New York"
- `en_demand_letter` p1 PersonName [heuristic]: "Because Mr"
- `en_demand_letter` p1 Address [llm]: "Kings County"
- `en_demo_pdf` p1 PersonName [heuristic]: "Contract No"
- `en_demo_pdf` p1 PersonName [heuristic]: "The Project Manager"
- `en_demo_pdf` p1 PersonName [heuristic]: "The Client"
- `en_demo_pdf` p1 Date [llm]: "Phase 1 deliverables"
- `en_demo_pdf` p2 PersonName [heuristic]: "Internal Revenue Service"
- `en_demo_pdf` p2 PersonName [heuristic]: "Confidential Information"
- `en_demo_pdf` p3 PersonName [heuristic]: "San Francisco"
- `en_demo_pdf` p3 PersonName [heuristic]: "American Arbitration Association"
- `en_demo_pdf` p3 PersonName [heuristic]: "Managing Partner"
- `en_demo_pdf` p3 PersonName [heuristic]: "Chief Executive Officer"
- `en_demo_pdf` p3 PersonName [heuristic]: "Senior Director"
- `en_eu_share_purchase` p1 PersonName [heuristic]: "Purchase Price"
- `en_eu_share_purchase` p1 PersonName [heuristic]: "Managing Director"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Judgment Creditor"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Judgment Debtor"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Recovery Manager"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Judgment Debtor"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "American Express"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Judgment Debtor"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Notary Public"
- `en_settlement_agreement` p1 PersonName [heuristic]: "This Confidential Settlement Agreement"
- `en_settlement_agreement` p1 PersonName [heuristic]: "The Employee"
- `en_settlement_agreement` p1 PersonName [heuristic]: "Data Incident"
- `en_settlement_agreement` p1 PersonName [heuristic]: "National Insurance"
- `en_settlement_agreement` p1 PersonName [heuristic]: "Confidential Information"
- `en_settlement_agreement` p1 PersonName [heuristic]: "Effective Date"
- `en_settlement_agreement` p1 PersonName [heuristic]: "Data Protection Officer"
- `en_settlement_agreement` p1 PersonName [heuristic]: "Neither Party"
- `en_traps_llc_agreement` p1 CompanyName [heuristic]: "Delaware Limited"
- `en_traps_llc_agreement` p1 PersonName [heuristic]: "Fair Market Value"
- `en_traps_llc_agreement` p1 PersonName [heuristic]: "Force Majeure Event"
- `en_traps_llc_agreement` p1 PersonName [heuristic]: "Internal Revenue Code"
- `en_traps_llc_agreement` p1 PersonName [heuristic]: "Preferred Return"
- `en_traps_llc_agreement` p1 PersonName [heuristic]: "Along Notice"
- `en_traps_llc_agreement` p1 PersonName [heuristic]: "Effective Date"
- `en_traps_llc_agreement` p1 PersonName [heuristic]: "The Board"
- `en_traps_llc_agreement` p1 CompanyName [heuristic]: "Walt Disney Co"
- `en_traps_llc_agreement` p1 PersonName [heuristic]: "Governing Law"
- `en_traps_llc_agreement` p1 PersonName [heuristic]: "Entire Agreement"
- `en_traps_llc_agreement` p1 PersonName [heuristic]: "Effective Date"
- `en_traps_llc_agreement` p1 PersonName [heuristic]: "Managing Director"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Company described in Article V"
- `en_traps_llc_agreement` p1 Address [llm]: "Schedule A"
- `en_traps_llc_agreement` p1 Address [llm]: "Fiscal Year 2025"
- `en_traps_llc_agreement` p1 NationalId [llm]: "965 A.2d 695"
- `en_traps_llc_agreement` p1 Date [llm]: "2009"
- `en_traps_llc_agreement` p1 Date [llm]: "2006"
- `en_traps_llc_agreement` p1 Address [llm]: "Exhibits and Schedules hereto"
- `en_traps_llc_agreement` p1 Address [llm]: "MANAGING MEMBER"
- `es_carta_requerimiento` p1 PersonName [heuristic]: "Ciudad de México"
- `es_carta_requerimiento` p1 PersonName [heuristic]: "Estimada Sra"
- `es_carta_requerimiento` p1 Address [llm]: "Procuraduría Federal del Consumidor (PROFECO)"
- `es_contrato_arrendamiento` p1 PersonName [heuristic]: "En Madrid"
- `es_contrato_arrendamiento` p1 PersonName [heuristic]: "Arrendamientos Urbanos"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Juicio Verbal"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Ley de Enjuiciamiento Civil"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Código Civil"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "En Valencia"
- `es_demanda_reclamacion` p1 CaseNumber [llm]: "5 DE VALENCIA"
- `holdout_en_intake_memo` p1 PersonName [heuristic]: "Litigation Team"
- `holdout_es_acta_junta` p1 PersonName [heuristic]: "En Bilbao"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Prezado Sr"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Ofício de Registro de Imóveis de Belo Horizonte"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Lei do Inquilinato"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Belo Horizonte"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "Banco do Brasil"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "Lei Geral de Proteção de Dados"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "Foro da Comarca de São Paulo"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "São Paulo"
- `pt_peticao_inicial` p1 PersonName [heuristic]: "Código de Processo Civil"
- `pt_peticao_inicial` p1 PersonName [heuristic]: "São Paulo"
- `pt_peticao_inicial` p1 Address [llm]: "SÃO PAULO"
- `pt_peticao_inicial` p1 CaseNumber [llm]: "1.737.412/SE"
- `pt_peticao_inicial` p1 CourtFileNumber [llm]: "37 do STJ"
- `pt_procuracao_portugal` p1 PersonName [heuristic]: "Caro Sr"
- `pt_procuracao_portugal` p1 Address [llm]: "Conservatória do Registo Predial do Porto"
- `pt_procuracao_portugal` p1 Address [llm]: "Rua de Santa Catarina"
- `pt_procuracao_portugal` p1 Address [llm]: "Cartório Notarial de Lisboa"

### Layer 3 alone false positives, run 1

- `en_demand_letter` p1 Address: "Attn: Claims Department"
- `en_demand_letter` p1 Address: "Kings County"
- `en_demo_pdf` p1 Date: "Phase 1 deliverables"
- `en_traps_llc_agreement` p1 CompanyName: "Company described in Article V"
- `en_traps_llc_agreement` p1 Address: "Schedule A"
- `en_traps_llc_agreement` p1 Address: "Fiscal Year 2025"
- `en_traps_llc_agreement` p1 NationalId: "965 A.2d 695"
- `en_traps_llc_agreement` p1 Date: "2009"
- `en_traps_llc_agreement` p1 Date: "2006"
- `en_traps_llc_agreement` p1 Address: "Exhibits and Schedules hereto"
- `en_traps_llc_agreement` p1 Address: "MANAGING MEMBER"
- `es_carta_requerimiento` p1 Address: "Procuraduría Federal del Consumidor (PROFECO)"
- `es_demanda_reclamacion` p1 CaseNumber: "5 DE VALENCIA"
- `pt_peticao_inicial` p1 Address: "SÃO PAULO"
- `pt_peticao_inicial` p1 CaseNumber: "1.737.412/SE"
- `pt_peticao_inicial` p1 CourtFileNumber: "37 do STJ"
- `pt_peticao_inicial` p1 Address: "São Paulo"
- `pt_procuracao_portugal` p1 Address: "Conservatória do Registo Predial do Porto"
- `pt_procuracao_portugal` p1 Address: "Rua de Santa Catarina"
- `pt_procuracao_portugal` p1 Address: "Cartório Notarial de Lisboa"
