# Eval: Layer 3 with `qwen3-1.7b-q8_0`

assessFit: **likely-fits** · load 1.3 s · 18 pages · 3 run(s) · prompt `p3` · chunks of 150 words, 20 overlap · post-processing: default

## Overall (overlap matching; mean ± sd over runs)

| | P | R | F1 | covered R | EN F1 | ES F1 | PT F1 |
|---|---|---|---|---|---|---|---|
| Layer 3 alone | 97.2 ± 0.0 | 67.0 ± 0.0 | 79.3 ± 0.0 | 66.2 ± 0.0 | 75.5 ± 0.0 | 82.2 ± 0.0 | 85.7 ± 0.0 |
| Layers 1+2 | 84.9 | 83.1 | 84.0 | 75.5 | 80.0 | 88.2 | 90.2 |
| Layers 1+2+3 | 84.9 ± 0.0 | 93.1 ± 0.0 | 88.8 ± 0.0 | 85.0 ± 0.0 | 87.1 ± 0.0 | 89.7 ± 0.0 | 92.5 ± 0.0 |

Speed: 6.22 ± 0.00 s/page (Layer 3 only; 53 chunks/run, 0 failed chunks over all runs: 0 engine errors, 0 unparseable answers). Peak RSS of the sidecar: 2425 MB.

Run-to-run: span-set Jaccard 100.0 ± 0.0 (pairs of runs), identical raw answers 100.0 ± 0.0 of chunks.

### Exact / overlap / covered, run 1

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| Layer 3 alone | 461 | 319 | 97.2 / 67.0 / 79.3 | 88.7 / 61.4 / 72.6 | 66.2 | 92.6 |
| Layers 1+2+3 | 461 | 503 | 84.9 / 93.1 / 88.8 | 75.7 / 82.6 / 79.0 | 85.0 | 97.0 |
| Layers 1+2+3 – dev | 401 | 447 | 84.3 / 94.3 / 89.0 | 76.5 / 85.3 / 80.7 | 87.5 | 97.4 |
| Layers 1+2+3 – holdout | 60 | 56 | 89.3 / 85.0 / 87.1 | 69.6 / 65.0 / 67.2 | 68.3 | 94.1 |

### Prompt versions (run 1 each)

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| `p3` Layer 3 alone (run 1) | 461 | 319 | 97.2 / 67.0 / 79.3 | 88.7 / 61.4 / 72.6 | 66.2 | 92.6 |
| `p3` Layers 1+2+3 (run 1) | 461 | 503 | 84.9 / 93.1 / 88.8 | 75.7 / 82.6 / 79.0 | 85.0 | 97.0 |

### Post-processing ablation (replayed on run 1's answers)

| config | L3 P | L3 R | L3 F1 | L1+2+3 P | L1+2+3 R | L1+2+3 F1 |
|---|---|---|---|---|---|---|
| default | 97.2 | 67.0 | 79.3 | 84.9 | 93.1 | 88.8 |
| Phase 1B post-processing | 92.8 | 61.2 | 73.7 | 82.3 | 90.0 | 86.0 |
| − stop-list | 89.3 | 67.0 | 76.6 | 80.4 | 93.1 | 86.3 |
| − Last, First variants | 97.1 | 66.2 | 78.7 | 84.8 | 92.6 | 88.6 |
| − honorific + surname | 97.1 | 65.3 | 78.1 | 84.7 | 91.5 | 88.0 |
| − page-wide mapping | 97.3 | 63.1 | 76.6 | 84.9 | 91.8 | 88.2 |
| − accent folding | 97.2 | 67.0 | 79.3 | 84.9 | 93.1 | 88.8 |
| − hyphen line-wrap | 97.2 | 66.8 | 79.2 | 84.9 | 92.8 | 88.7 |

### What happened to the model's items (run 1)

```
{
  "items": 349,
  "salvaged_chunks": 0,
  "invalid_items": 0,
  "unknown_category": 0,
  "empty_text": 0,
  "implausible": 27,
  "stoplisted": 6,
  "not_in_chunk": 6,
  "accepted": 309,
  "variant_hits": 14,
  "page_wide_hits": 31,
  "covered_by_layers_1_2": 0
}
```

### Layer 3 alone by category (all runs pooled; for confidence calibration)

| category | preds | P (overlap) | R (overlap) | type acc. |
|---|---:|---:|---:|---:|
| Address | 96 | 84.4 | 89.7 | 100.0 |
| BankAccount | 54 | 100.0 | 60.0 | 100.0 |
| CaseNumber | 36 | 75.0 | 50.0 | 100.0 |
| ClientId | 3 | 100.0 | 80.0 | 25.0 |
| CompanyName | 72 | 100.0 | 59.5 | 100.0 |
| CourtFileNumber | 0 | – | 100.0 | 0.0 |
| CreditCard | 9 | 100.0 | 75.0 | 50.0 |
| Date | 81 | 100.0 | 38.9 | 96.4 |
| DateOfBirth | 27 | 100.0 | 100.0 | 100.0 |
| DriversLicense | 3 | 100.0 | 50.0 | 50.0 |
| Email | 36 | 100.0 | 56.0 | 85.7 |
| IBAN | 6 | 100.0 | 100.0 | 22.2 |
| IpAddress | 12 | 100.0 | 66.7 | 100.0 |
| LicensePlate | 18 | 100.0 | 100.0 | 100.0 |
| MedicalRecordNumber | 6 | 100.0 | 50.0 | 100.0 |
| MonetaryAmount | 111 | 100.0 | 82.6 | 97.4 |
| NationalId | 54 | 100.0 | 88.2 | 100.0 |
| PassportNumber | 3 | 100.0 | 33.3 | 100.0 |
| PersonName | 246 | 98.8 | 69.9 | 100.0 |
| PhoneNumber | 63 | 100.0 | 84.0 | 100.0 |
| SocialSecurityNumber | 3 | 100.0 | 66.7 | 50.0 |
| TaxId | 12 | 100.0 | 43.8 | 57.1 |
| Url | 6 | 100.0 | 66.7 | 100.0 |

Agreement (run 1): detections confirmed by two overlapping chunks: 43/43 correct (100.0); single-chunk: 267/276 (96.7).

### Layer 3 additions to Layers 1+2 by category (all runs pooled; confidence calibration)

| category | added | correct | P |
|---|---:|---:|---:|
| Address | 15 | 3 | 20.0 |
| BankAccount | 6 | 6 | 100.0 |
| CaseNumber | 12 | 3 | 25.0 |
| CompanyName | 15 | 15 | 100.0 |
| Date | 9 | 9 | 100.0 |
| DateOfBirth | 3 | 3 | 100.0 |
| NationalId | 3 | 3 | 100.0 |
| PersonName | 99 | 96 | 97.0 |
| **all** | 162 | 138 | 85.2 |

### Layers 1+2+3 by category, run 1

| category | labels | L1+2 P / R | L3 P / R | L1+2+3 P / R |
|---|---:|---|---|---|
| Address | 29 | 100.0 / 89.7 | 84.4 / 89.7 | 87.1 / 93.1 |
| BankAccount | 15 | 100.0 / 73.3 | 100.0 / 60.0 | 100.0 / 86.7 |
| CaseNumber | 6 | 100.0 / 83.3 | 75.0 / 50.0 | 66.7 / 83.3 |
| ClientId | 5 | 100.0 / 80.0 | 100.0 / 80.0 | 100.0 / 100.0 |
| CompanyName | 37 | 92.0 / 81.1 | 100.0 / 59.5 | 93.3 / 91.9 |
| CourtFileNumber | 1 | 100.0 / 100.0 | – / 100.0 | 100.0 / 100.0 |
| CreditCard | 8 | 100.0 / 100.0 | 100.0 / 75.0 | 100.0 / 100.0 |
| Date | 72 | 100.0 / 91.7 | 100.0 / 38.9 | 100.0 / 95.8 |
| DateOfBirth | 9 | 100.0 / 88.9 | 100.0 / 100.0 | 100.0 / 100.0 |
| DriversLicense | 4 | 100.0 / 100.0 | 100.0 / 50.0 | 100.0 / 100.0 |
| Email | 25 | 100.0 / 96.0 | 100.0 / 56.0 | 100.0 / 100.0 |
| IBAN | 9 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| IpAddress | 6 | 100.0 / 100.0 | 100.0 / 66.7 | 100.0 / 100.0 |
| LicensePlate | 5 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| MedicalRecordNumber | 4 | 100.0 / 100.0 | 100.0 / 50.0 | 100.0 / 100.0 |
| MonetaryAmount | 46 | 100.0 / 100.0 | 100.0 / 82.6 | 100.0 / 100.0 |
| NationalId | 17 | 100.0 / 88.2 | 100.0 / 88.2 | 100.0 / 94.1 |
| PassportNumber | 3 | 100.0 / 100.0 | 100.0 / 33.3 | 100.0 / 100.0 |
| PersonName | 113 | 50.0 / 54.0 | 98.8 / 69.9 | 59.4 / 82.3 |
| PhoneNumber | 25 | 100.0 / 100.0 | 100.0 / 84.0 | 100.0 / 100.0 |
| SocialSecurityNumber | 3 | 100.0 / 100.0 | 100.0 / 66.7 | 100.0 / 100.0 |
| TaxId | 16 | 100.0 / 100.0 | 100.0 / 43.8 | 100.0 / 100.0 |
| Url | 3 | 100.0 / 100.0 | 100.0 / 66.7 | 100.0 / 100.0 |

### Layers 1+2+3 errors, run 1

False negatives (69):

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
- `en_demo_pdf` p1 Email (partial): "j.richardson@atlas-  \n  digital.com"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 Address (missed): "Westerdok 118, 1013 BH Amsterdam"
- `en_eu_share_purchase` p1 PersonName (missed): "Felix Brandauer"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine France SAS"
- `en_eu_share_purchase` p1 CaseNumber (missed): "16 O 248/24"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 PersonName (missed): "Joost van der Linde"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 CompanyName (missed): "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 PersonName (missed): "Joost van der Linde"
- `en_garnishment_affidavit` p1 CompanyName (missed): "Desert Sun Credit Union"
- `en_garnishment_affidavit` p1 PersonName (missed): "LORNA FETTERMAN"
- `en_garnishment_affidavit` p1 CompanyName (missed): "Desert Sun Credit Union"
- `en_garnishment_affidavit` p1 BankAccount (missed): "884 120 33571"
- `en_garnishment_affidavit` p1 BankAccount (missed): "3266 0519 8820"
- `en_settlement_agreement` p1 PersonName (missed): "Rafferty\nOkonkwo-Bell"
- `en_settlement_agreement` p1 PersonName (partial): "Ms Achterberg"
- `en_traps_llc_agreement` p1 PersonName (partial): "Constance Varga-Ellison"
- `es_carta_requerimiento` p1 CompanyName (partial): "Bufete Arriaga & Solís, S.C."
- `es_carta_requerimiento` p1 PersonName (missed): "Ramírez de la Garza"
- `es_carta_requerimiento` p1 PersonName (missed): "J. M. Ortuño"
- `es_contrato_arrendamiento` p1 PersonName (partial): "Tomás Ezequiel Barroso\nQuintanilla"
- `es_contrato_arrendamiento` p1 CompanyName (partial): "Inversiones Quintanilla Hermanos, S.L."
- `es_contrato_arrendamiento` p1 PersonName (missed): "Rogelio Quintanilla Mateu"
- `es_contrato_arrendamiento` p1 PersonName (partial): "Tomás E. Barroso Quintanilla"
- `es_demanda_reclamacion` p1 CompanyName (partial): "Reformas Turia Integral, S.L.U."
- `es_demanda_reclamacion` p1 PersonName (missed): "Aleixandre"
- `es_demanda_reclamacion` p1 PersonName (missed): "Álvaro Sendra Peiró"
- `holdout_en_intake_memo` p1 PersonName (partial): "Naomi Fairbanks-Oyelaran"
- `holdout_en_intake_memo` p1 Date (missed): "2025.03.18"
- `holdout_en_intake_memo` p1 PersonName (partial): "Desmond Achebe-Kowalczyk"
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
- `holdout_pt_notificacao` p1 NationalId (missed): "39053344705"
- `pt_contrato_prestacao` p1 PersonName (partial): "Helena Cristina dos Santos Albuquerque"
- `pt_contrato_prestacao` p1 PersonName (partial): "Priscila\nFagundes Teixeira"
- `pt_contrato_prestacao` p1 CompanyName (partial): "LATICÍNIOS SERRA AZUL LTDA."
- `pt_contrato_prestacao` p1 PersonName (partial): "Helena Cristina dos Santos Albuquerque"
- `pt_peticao_inicial` p1 PersonName (partial): "CARVALHO, Juliana Rezende de"
- `pt_peticao_inicial` p1 PersonName (partial): "Edvaldo\nNogueira Paixão"
- `pt_peticao_inicial` p1 PersonName (missed): "Paixão"
- `pt_procuracao_portugal` p1 CompanyName (partial): "Douro Vinhos & Quintas, Lda."
- `pt_procuracao_portugal` p1 PersonName (missed): "J. M. T. Fonseca"

False positives (76):

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
- `en_settlement_agreement` p1 PersonName [llm]: "Achterberg"
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
- `en_traps_llc_agreement` p1 CaseNumber [llm]: "Gantler v. Stephens, 965 A.2d 695 (Del. 2009)"
- `es_carta_requerimiento` p1 PersonName [heuristic]: "Ciudad de México"
- `es_carta_requerimiento` p1 PersonName [heuristic]: "Estimada Sra"
- `es_contrato_arrendamiento` p1 PersonName [heuristic]: "En Madrid"
- `es_contrato_arrendamiento` p1 PersonName [heuristic]: "Arrendamientos Urbanos"
- `es_contrato_arrendamiento` p1 Address [llm]: "CUARTA. PLAZA DE GARAJE"
- `es_contrato_arrendamiento` p1 Address [llm]: "la plaza de garaje n.º 14"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Juicio Verbal"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Ley de Enjuiciamiento Civil"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Código Civil"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "En Valencia"
- `es_demanda_reclamacion` p1 CaseNumber [llm]: "2024/0031"
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
- `pt_peticao_inicial` p1 CaseNumber [llm]: "STJ, REsp 1.737.412/SE"
- `pt_procuracao_portugal` p1 PersonName [heuristic]: "Caro Sr"

### Layer 3 alone false positives, run 1

- `en_court_complaint` p1 Address: "Contra Costa County"
- `en_settlement_agreement` p1 PersonName: "Achterberg"
- `en_traps_llc_agreement` p1 CaseNumber: "Gantler v. Stephens, 965 A.2d 695 (Del. 2009)"
- `es_contrato_arrendamiento` p1 Address: "CUARTA. PLAZA DE GARAJE"
- `es_contrato_arrendamiento` p1 Address: "la plaza de garaje n.º 14"
- `es_demanda_reclamacion` p1 CaseNumber: "2024/0031"
- `pt_peticao_inicial` p1 Address: "SÃO PAULO"
- `pt_peticao_inicial` p1 CaseNumber: "STJ, REsp 1.737.412/SE"
- `pt_peticao_inicial` p1 Address: "São Paulo"
