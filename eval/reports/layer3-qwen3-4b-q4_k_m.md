# Eval: Layer 3 with `qwen3-4b-q4_k_m`

assessFit: **likely-fits** · load 1.6 s · 18 pages · 3 run(s) · prompt `p3` · chunks of 150 words, 20 overlap · post-processing: default

## Overall (overlap matching; mean ± sd over runs)

| | P | R | F1 | covered R | EN F1 | ES F1 | PT F1 |
|---|---|---|---|---|---|---|---|
| Layer 3 alone | 98.3 ± 0.0 | 76.8 ± 0.0 | 86.2 ± 0.0 | 76.1 ± 0.0 | 85.6 ± 0.0 | 88.5 ± 0.0 | 85.7 ± 0.0 |
| Layers 1+2 | 84.9 | 83.1 | 84.0 | 75.5 | 80.0 | 88.2 | 90.2 |
| Layers 1+2+3 | 85.4 ± 0.0 | 94.4 ± 0.0 | 89.7 ± 0.0 | 86.6 ± 0.0 | 88.1 ± 0.0 | 90.6 ± 0.0 | 93.0 ± 0.0 |

Speed: 12.05 ± 0.00 s/page (Layer 3 only; 53 chunks/run, 0 failed chunks over all runs: 0 engine errors, 0 unparseable answers). Peak RSS of the sidecar: 3081 MB.

Run-to-run: span-set Jaccard 100.0 ± 0.0 (pairs of runs), identical raw answers 100.0 ± 0.0 of chunks.

### Exact / overlap / covered, run 1

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| Layer 3 alone | 461 | 358 | 98.3 / 76.8 / 86.2 | 91.3 / 70.9 / 79.9 | 76.1 | 95.8 |
| Layers 1+2+3 | 461 | 507 | 85.4 / 94.4 / 89.7 | 76.3 / 83.9 / 80.0 | 86.6 | 97.2 |
| Layers 1+2+3 – dev | 401 | 451 | 85.1 / 96.0 / 90.3 | 77.2 / 86.8 / 81.7 | 89.5 | 97.4 |
| Layers 1+2+3 – holdout | 60 | 56 | 87.5 / 83.3 / 85.4 | 69.6 / 65.0 / 67.2 | 66.7 | 96.0 |

### Prompt versions (run 1 each)

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| `p3` Layer 3 alone (run 1) | 461 | 358 | 98.3 / 76.8 / 86.2 | 91.3 / 70.9 / 79.9 | 76.1 | 95.8 |
| `p3` Layers 1+2+3 (run 1) | 461 | 507 | 85.4 / 94.4 / 89.7 | 76.3 / 83.9 / 80.0 | 86.6 | 97.2 |
| Phase 1B (p1b prompt + p1b post) Layer 3 alone | 461 | 273 | 98.2 / 58.1 / 73.0 | 92.3 / 54.7 / 68.7 | 57.7 | 90.7 |
| Phase 1B (p1b prompt + p1b post) Layers 1+2+3 | 461 | 482 | 84.9 / 89.2 / 87.0 | 76.1 / 79.6 / 77.8 | 81.6 | 96.8 |
| `p1b` Layer 3 alone (run 1) | 461 | 305 | 98.4 / 65.1 / 78.3 | 92.8 / 61.4 / 73.9 | 64.4 | 91.7 |
| `p1b` Layers 1+2+3 (run 1) | 461 | 503 | 85.5 / 93.7 / 89.4 | 76.9 / 83.9 / 80.3 | 85.9 | 97.0 |

### Post-processing ablation (replayed on run 1's answers)

| config | L3 P | L3 R | L3 F1 | L1+2+3 P | L1+2+3 R | L1+2+3 F1 |
|---|---|---|---|---|---|---|
| default | 98.3 | 76.8 | 86.2 | 85.4 | 94.4 | 89.7 |
| Phase 1B post-processing | 98.2 | 71.4 | 82.7 | 84.9 | 90.5 | 87.6 |
| − stop-list | 98.3 | 76.8 | 86.2 | 85.4 | 94.4 | 89.7 |
| − Last, First variants | 98.3 | 75.5 | 85.4 | 85.3 | 93.5 | 89.2 |
| − honorific + surname | 98.3 | 75.1 | 85.1 | 85.2 | 92.6 | 88.7 |
| − page-wide mapping | 98.3 | 73.8 | 84.3 | 85.2 | 92.6 | 88.7 |
| − accent folding | 98.3 | 76.8 | 86.2 | 85.4 | 94.4 | 89.7 |
| − hyphen line-wrap | 98.3 | 76.6 | 86.1 | 85.4 | 94.1 | 89.5 |

### What happened to the model's items (run 1)

```
{
  "items": 371,
  "salvaged_chunks": 0,
  "invalid_items": 0,
  "unknown_category": 0,
  "empty_text": 0,
  "implausible": 5,
  "stoplisted": 0,
  "not_in_chunk": 7,
  "accepted": 359,
  "variant_hits": 17,
  "page_wide_hits": 29,
  "covered_by_layers_1_2": 0
}
```

### Layer 3 alone by category (all runs pooled; for confidence calibration)

| category | preds | P (overlap) | R (overlap) | type acc. |
|---|---:|---:|---:|---:|
| Address | 69 | 95.7 | 75.9 | 100.0 |
| BankAccount | 27 | 100.0 | 60.0 | 88.9 |
| CaseNumber | 27 | 77.8 | 100.0 | 83.3 |
| ClientId | 9 | 100.0 | 80.0 | 50.0 |
| CompanyName | 81 | 100.0 | 73.0 | 100.0 |
| CourtFileNumber | 0 | – | 100.0 | 0.0 |
| CreditCard | 24 | 100.0 | 87.5 | 100.0 |
| Date | 126 | 100.0 | 59.7 | 100.0 |
| DateOfBirth | 27 | 100.0 | 100.0 | 100.0 |
| DriversLicense | 9 | 100.0 | 75.0 | 100.0 |
| Email | 72 | 100.0 | 96.0 | 100.0 |
| IBAN | 12 | 100.0 | 77.8 | 57.1 |
| IpAddress | 18 | 100.0 | 83.3 | 100.0 |
| LicensePlate | 18 | 83.3 | 100.0 | 100.0 |
| MedicalRecordNumber | 9 | 100.0 | 100.0 | 75.0 |
| MonetaryAmount | 123 | 100.0 | 89.1 | 100.0 |
| NationalId | 63 | 100.0 | 94.1 | 100.0 |
| PassportNumber | 3 | 100.0 | 100.0 | 33.3 |
| PersonName | 222 | 100.0 | 65.5 | 100.0 |
| PhoneNumber | 72 | 100.0 | 100.0 | 96.0 |
| SocialSecurityNumber | 9 | 100.0 | 100.0 | 100.0 |
| TaxId | 45 | 86.7 | 81.2 | 76.9 |
| Url | 9 | 100.0 | 100.0 | 100.0 |

Agreement (run 1): detections confirmed by two overlapping chunks: 52/52 correct (100.0); single-chunk: 300/306 (98.0).

### Layer 3 additions to Layers 1+2 by category (all runs pooled; confidence calibration)

| category | added | correct | P |
|---|---:|---:|---:|
| Address | 9 | 6 | 66.7 |
| BankAccount | 9 | 9 | 100.0 |
| CaseNumber | 9 | 3 | 33.3 |
| CompanyName | 18 | 18 | 100.0 |
| Date | 9 | 9 | 100.0 |
| DateOfBirth | 3 | 3 | 100.0 |
| Email | 3 | 3 | 100.0 |
| LicensePlate | 3 | 0 | 0.0 |
| NationalId | 9 | 9 | 100.0 |
| PersonName | 96 | 96 | 100.0 |
| TaxId | 6 | 0 | 0.0 |
| **all** | 174 | 156 | 89.7 |

### Layers 1+2+3 by category, run 1

| category | labels | L1+2 P / R | L3 P / R | L1+2+3 P / R |
|---|---:|---|---|---|
| Address | 29 | 100.0 / 89.7 | 95.7 / 75.9 | 96.6 / 96.6 |
| BankAccount | 15 | 100.0 / 73.3 | 100.0 / 60.0 | 100.0 / 100.0 |
| CaseNumber | 6 | 100.0 / 83.3 | 77.8 / 100.0 | 75.0 / 100.0 |
| ClientId | 5 | 100.0 / 80.0 | 100.0 / 80.0 | 100.0 / 80.0 |
| CompanyName | 37 | 92.0 / 81.1 | 100.0 / 73.0 | 93.5 / 97.3 |
| CourtFileNumber | 1 | 100.0 / 100.0 | – / 100.0 | 100.0 / 100.0 |
| CreditCard | 8 | 100.0 / 100.0 | 100.0 / 87.5 | 100.0 / 100.0 |
| Date | 72 | 100.0 / 91.7 | 100.0 / 59.7 | 100.0 / 95.8 |
| DateOfBirth | 9 | 100.0 / 88.9 | 100.0 / 100.0 | 100.0 / 100.0 |
| DriversLicense | 4 | 100.0 / 100.0 | 100.0 / 75.0 | 100.0 / 100.0 |
| Email | 25 | 100.0 / 96.0 | 100.0 / 96.0 | 100.0 / 100.0 |
| IBAN | 9 | 100.0 / 100.0 | 100.0 / 77.8 | 100.0 / 100.0 |
| IpAddress | 6 | 100.0 / 100.0 | 100.0 / 83.3 | 100.0 / 100.0 |
| LicensePlate | 5 | 100.0 / 100.0 | 83.3 / 100.0 | 83.3 / 100.0 |
| MedicalRecordNumber | 4 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| MonetaryAmount | 46 | 100.0 / 100.0 | 100.0 / 89.1 | 100.0 / 100.0 |
| NationalId | 17 | 100.0 / 88.2 | 100.0 / 94.1 | 100.0 / 100.0 |
| PassportNumber | 3 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| PersonName | 113 | 50.0 / 54.0 | 100.0 / 65.5 | 59.8 / 82.3 |
| PhoneNumber | 25 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| SocialSecurityNumber | 3 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| TaxId | 16 | 100.0 / 100.0 | 86.7 / 81.2 | 90.9 / 100.0 |
| Url | 3 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |

### Layers 1+2+3 errors, run 1

False negatives (62):

- `en_court_complaint` p1 PersonName (partial): "Marcus T. Feeney"
- `en_court_complaint` p1 PersonName (missed): "Priya\nRamaswamy"
- `en_court_complaint` p1 PersonName (missed): "Ramaswamy"
- `en_court_complaint` p1 PersonName (missed): "Ellen K. Marsh"
- `en_court_complaint` p1 CompanyName (partial): "Marsh & Okonjo LLP"
- `en_demand_letter` p1 CompanyName (partial): "Okafor Whitfield PLLC"
- `en_demand_letter` p1 PersonName (partial): "Harriet Pemberton-Shaw"
- `en_demand_letter` p1 ClientId (partial): "NGM-24-0073318"
- `en_demand_letter` p1 MedicalRecordNumber (partial): "MRN 00482913"
- `en_demand_letter` p1 PersonName (missed): "T. R. Lindqvist"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 PersonName (missed): "Felix Brandauer"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine France SAS"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_garnishment_affidavit` p1 PersonName (missed): "LORNA FETTERMAN"
- `en_settlement_agreement` p1 PersonName (missed): "S. Achterberg"
- `en_settlement_agreement` p1 PersonName (missed): "Achterberg"
- `en_settlement_agreement` p1 PersonName (missed): "Rafferty\nOkonkwo-Bell"
- `en_settlement_agreement` p1 PersonName (partial): "Ms Achterberg"
- `en_traps_llc_agreement` p1 PersonName (missed): "Rupert Ashdown"
- `en_traps_llc_agreement` p1 PersonName (partial): "Constance Varga-Ellison"
- `es_carta_requerimiento` p1 CompanyName (partial): "Bufete Arriaga & Solís, S.C."
- `es_carta_requerimiento` p1 PersonName (missed): "Ramírez de la Garza"
- `es_contrato_arrendamiento` p1 PersonName (partial): "Tomás Ezequiel Barroso\nQuintanilla"
- `es_contrato_arrendamiento` p1 CompanyName (partial): "Inversiones Quintanilla Hermanos, S.L."
- `es_contrato_arrendamiento` p1 PersonName (missed): "Rogelio Quintanilla Mateu"
- `es_contrato_arrendamiento` p1 PersonName (partial): "Tomás E. Barroso Quintanilla"
- `es_demanda_reclamacion` p1 CompanyName (partial): "Reformas Turia Integral, S.L.U."
- `es_demanda_reclamacion` p1 PersonName (missed): "Fermín Aleixandre Bou"
- `es_demanda_reclamacion` p1 PersonName (missed): "Aleixandre"
- `es_demanda_reclamacion` p1 PersonName (missed): "Álvaro Sendra Peiró"
- `holdout_en_intake_memo` p1 PersonName (partial): "Naomi Fairbanks-Oyelaran"
- `holdout_en_intake_memo` p1 Date (missed): "2025.03.18"
- `holdout_en_intake_memo` p1 PersonName (partial): "Desmond Achebe-Kowalczyk"
- `holdout_en_intake_memo` p1 CompanyName (missed): "Pellucid Freight Systems, Inc."
- `holdout_en_intake_memo` p1 ClientId (missed): "M-25-0331"
- `holdout_en_intake_memo` p1 PersonName (partial): "Desmond Achebe-Kowalczyk"
- `holdout_en_intake_memo` p1 Address (partial): "83 Lantern Hill Terrace, Unit 12, Marietta, GA 30064-3317"
- `holdout_en_intake_memo` p1 Address (missed): "2210 Rue Sainte-Catherine Ouest, Montréal, QC H3H 1M7"
- `holdout_en_intake_memo` p1 PersonName (missed): "Desmond"
- `holdout_en_intake_memo` p1 Date (missed): "Feb. 3, 2021"
- `holdout_en_intake_memo` p1 Date (missed): "Nov. 30, 2024"
- `holdout_en_intake_memo` p1 CompanyName (partial): "Quist Harrow & Lyle LLP"
- `holdout_es_acta_junta` p1 PersonName (partial): "Íñigo Zabaleta Urrutia"
- `holdout_es_acta_junta` p1 PersonName (missed): "Zabaleta"
- `holdout_es_acta_junta` p1 PersonName (missed): "Beloki"
- `holdout_es_acta_junta` p1 CompanyName (partial): "Auditores Ibaizabal, S.L.P."
- `holdout_es_acta_junta` p1 PersonName (missed): "Koldo Iturbe Larrañaga"
- `holdout_es_acta_junta` p1 Address (partial): "Alameda de Recalde, 27, 5.º dcha., 48009 Bilbao"
- `holdout_es_acta_junta` p1 PhoneNumber (partial): "0034 944 555 120"
- `holdout_es_acta_junta` p1 PersonName (partial): "Íñigo Zabaleta Urrutia"
- `pt_contrato_prestacao` p1 PersonName (partial): "Helena Cristina dos Santos Albuquerque"
- `pt_contrato_prestacao` p1 PersonName (partial): "Priscila\nFagundes Teixeira"
- `pt_contrato_prestacao` p1 CompanyName (partial): "LATICÍNIOS SERRA AZUL LTDA."
- `pt_contrato_prestacao` p1 PersonName (partial): "Helena Cristina dos Santos Albuquerque"
- `pt_peticao_inicial` p1 PersonName (partial): "CARVALHO, Juliana Rezende de"
- `pt_peticao_inicial` p1 PersonName (partial): "Edvaldo\nNogueira Paixão"
- `pt_procuracao_portugal` p1 CompanyName (partial): "Douro Vinhos & Quintas, Lda."
- `pt_procuracao_portugal` p1 PersonName (missed): "J. M. T. Fonseca"

False positives (74):

- `en_court_complaint` p1 PersonName [heuristic]: "See Burlington"
- `en_court_complaint` p1 PersonName [heuristic]: "Civil Rights Act"
- `en_court_complaint` p1 Address [llm]: "Contra Costa County"
- `en_demand_letter` p1 PersonName [heuristic]: "Claims Department"
- `en_demand_letter` p1 PersonName [heuristic]: "Claim No"
- `en_demand_letter` p1 PersonName [heuristic]: "Dear Claims Representative"
- `en_demand_letter` p1 PersonName [heuristic]: "New York"
- `en_demand_letter` p1 PersonName [heuristic]: "Because Mr"
- `en_demo_pdf` p1 PersonName [heuristic]: "Contract No"
- `en_demo_pdf` p1 PersonName [heuristic]: "The Project Manager"
- `en_demo_pdf` p1 PersonName [heuristic]: "The Client"
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
- `es_carta_requerimiento` p1 PersonName [heuristic]: "Ciudad de México"
- `es_carta_requerimiento` p1 PersonName [heuristic]: "Estimada Sra"
- `es_contrato_arrendamiento` p1 PersonName [heuristic]: "En Madrid"
- `es_contrato_arrendamiento` p1 PersonName [heuristic]: "Arrendamientos Urbanos"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Juicio Verbal"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Ley de Enjuiciamiento Civil"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Código Civil"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "En Valencia"
- `es_demanda_reclamacion` p1 TaxId [llm]: "2024/0031"
- `holdout_en_intake_memo` p1 PersonName [heuristic]: "Litigation Team"
- `holdout_en_intake_memo` p1 LicensePlate [llm]: "1117"
- `holdout_es_acta_junta` p1 PersonName [heuristic]: "En Bilbao"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Prezado Sr"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Ofício de Registro de Imóveis de Belo Horizonte"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Lei do Inquilinato"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Belo Horizonte"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "Banco do Brasil"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "Lei Geral de Proteção de Dados"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "Foro da Comarca de São Paulo"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "São Paulo"
- `pt_contrato_prestacao` p1 CaseNumber [llm]: "10.406/2002"
- `pt_peticao_inicial` p1 PersonName [heuristic]: "Código de Processo Civil"
- `pt_peticao_inicial` p1 PersonName [heuristic]: "São Paulo"
- `pt_peticao_inicial` p1 CaseNumber [llm]: "1.737.412/SE"
- `pt_procuracao_portugal` p1 PersonName [heuristic]: "Caro Sr"
- `pt_procuracao_portugal` p1 TaxId [llm]: "23%"

### Layer 3 alone false positives, run 1

- `en_court_complaint` p1 Address: "Contra Costa County"
- `es_demanda_reclamacion` p1 TaxId: "2024/0031"
- `holdout_en_intake_memo` p1 LicensePlate: "1117"
- `pt_contrato_prestacao` p1 CaseNumber: "10.406/2002"
- `pt_peticao_inicial` p1 CaseNumber: "1.737.412/SE"
- `pt_procuracao_portugal` p1 TaxId: "23%"
