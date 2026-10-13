# Eval: Layer 3 with `qwen2.5-1.5b-instruct-q4_k_m`

assessFit: **likely-fits** · load 1.1 s · 18 pages · 3 run(s) · prompt `p3` · chunks of 150 words, 20 overlap · post-processing: default

## Overall (overlap matching; mean ± sd over runs)

| | P | R | F1 | covered R | EN F1 | ES F1 | PT F1 |
|---|---|---|---|---|---|---|---|
| Layer 3 alone | 90.3 ± 0.0 | 52.7 ± 0.0 | 66.6 ± 0.0 | 50.8 ± 0.0 | 67.9 ± 0.0 | 70.4 ± 0.0 | 58.7 ± 0.0 |
| Layers 1+2 | 84.9 | 83.1 | 84.0 | 75.5 | 80.0 | 88.2 | 90.2 |
| Layers 1+2+3 | 83.4 ± 0.0 | 95.2 ± 0.0 | 88.9 ± 0.0 | 87.2 ± 0.0 | 88.0 ± 0.0 | 88.9 ± 0.0 | 91.5 ± 0.0 |

Speed: 8.09 ± 0.21 s/page (Layer 3 only; 53 chunks/run, 0 failed chunks over all runs: 0 engine errors, 0 unparseable answers). Peak RSS of the sidecar: 1558 MB.

Run-to-run: span-set Jaccard 100.0 ± 0.0 (pairs of runs), identical raw answers 100.0 ± 0.0 of chunks.

### Exact / overlap / covered, run 1

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| Layer 3 alone | 461 | 268 | 90.3 / 52.7 / 66.6 | 78.0 / 45.3 / 57.3 | 50.8 | 81.9 |
| Layers 1+2+3 | 461 | 524 | 83.4 / 95.2 / 88.9 | 74.4 / 84.6 / 79.2 | 87.2 | 96.8 |
| Layers 1+2+3 – dev | 401 | 471 | 83.0 / 97.8 / 89.8 | 75.4 / 88.5 / 81.4 | 91.0 | 96.9 |
| Layers 1+2+3 – holdout | 60 | 53 | 86.8 / 78.3 / 82.3 | 66.0 / 58.3 / 61.9 | 61.7 | 95.7 |

### Prompt versions (run 1 each)

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| `p3` Layer 3 alone (run 1) | 461 | 268 | 90.3 / 52.7 / 66.6 | 78.0 / 45.3 / 57.3 | 50.8 | 81.9 |
| `p3` Layers 1+2+3 (run 1) | 461 | 524 | 83.4 / 95.2 / 88.9 | 74.4 / 84.6 / 79.2 | 87.2 | 96.8 |
| Phase 1B (p1b prompt + p1b post) Layer 3 alone | 461 | 185 | 90.8 / 36.4 / 52.0 | 82.7 / 33.2 / 47.4 | 35.8 | 76.8 |
| Phase 1B (p1b prompt + p1b post) Layers 1+2+3 | 461 | 490 | 83.5 / 89.2 / 86.2 | 74.7 / 79.4 / 77.0 | 81.6 | 96.4 |
| `p1b` Layer 3 alone (run 1) | 461 | 206 | 92.2 / 41.2 / 57.0 | 82.0 / 36.7 / 50.7 | 39.7 | 78.9 |
| `p1b` Layers 1+2+3 (run 1) | 461 | 503 | 84.3 / 92.4 / 88.2 | 75.0 / 81.8 / 78.2 | 84.4 | 96.2 |

### Post-processing ablation (replayed on run 1's answers)

| config | L3 P | L3 R | L3 F1 | L1+2+3 P | L1+2+3 R | L1+2+3 F1 |
|---|---|---|---|---|---|---|
| default | 90.3 | 52.7 | 66.6 | 83.4 | 95.2 | 88.9 |
| Phase 1B post-processing | 87.8 | 48.6 | 62.6 | 82.0 | 92.2 | 86.8 |
| − stop-list | 85.2 | 52.7 | 65.1 | 81.2 | 95.2 | 87.7 |
| − Last, First variants | 90.2 | 52.3 | 66.2 | 83.3 | 94.8 | 88.7 |
| − honorific + surname | 90.1 | 51.4 | 65.5 | 83.2 | 94.1 | 88.4 |
| − page-wide mapping | 90.3 | 51.0 | 65.2 | 83.3 | 94.6 | 88.6 |
| − accent folding | 90.3 | 52.7 | 66.6 | 83.4 | 95.2 | 88.9 |
| − hyphen line-wrap | 90.3 | 52.7 | 66.6 | 83.4 | 95.2 | 88.9 |

### What happened to the model's items (run 1)

```
{
  "items": 589,
  "salvaged_chunks": 22,
  "invalid_items": 0,
  "unknown_category": 0,
  "empty_text": 0,
  "implausible": 28,
  "stoplisted": 3,
  "not_in_chunk": 36,
  "accepted": 253,
  "variant_hits": 14,
  "page_wide_hits": 32,
  "covered_by_layers_1_2": 0
}
```

### Layer 3 alone by category (all runs pooled; for confidence calibration)

| category | preds | P (overlap) | R (overlap) | type acc. |
|---|---:|---:|---:|---:|
| Address | 99 | 90.9 | 65.5 | 100.0 |
| BankAccount | 36 | 100.0 | 46.7 | 100.0 |
| CaseNumber | 27 | 11.1 | 33.3 | 50.0 |
| ClientId | 3 | 100.0 | 0.0 | 100.0 |
| CompanyName | 48 | 100.0 | 73.0 | 55.6 |
| CourtFileNumber | 0 | – | 0.0 | 100.0 |
| CreditCard | 0 | – | 25.0 | 0.0 |
| Date | 42 | 85.7 | 27.8 | 60.0 |
| DateOfBirth | 36 | 100.0 | 55.6 | 100.0 |
| DriversLicense | 6 | 100.0 | 25.0 | 100.0 |
| Email | 24 | 100.0 | 40.0 | 80.0 |
| IBAN | 3 | 100.0 | 44.4 | 25.0 |
| IpAddress | 0 | – | 50.0 | 0.0 |
| LicensePlate | 0 | – | 0.0 | 100.0 |
| MedicalRecordNumber | 0 | – | 50.0 | 0.0 |
| MonetaryAmount | 51 | 88.2 | 28.3 | 100.0 |
| NationalId | 36 | 100.0 | 52.9 | 100.0 |
| PassportNumber | 0 | – | 66.7 | 0.0 |
| PersonName | 351 | 90.6 | 83.2 | 98.9 |
| PhoneNumber | 36 | 100.0 | 60.0 | 86.7 |
| SocialSecurityNumber | 6 | 100.0 | 66.7 | 100.0 |
| TaxId | 0 | – | 25.0 | 0.0 |
| Url | 0 | – | 66.7 | 0.0 |

Agreement (run 1): detections confirmed by two overlapping chunks: 64/64 correct (100.0); single-chunk: 178/204 (87.3).

### Layer 3 additions to Layers 1+2 by category (all runs pooled; confidence calibration)

| category | added | correct | P |
|---|---:|---:|---:|
| Address | 9 | 6 | 66.7 |
| BankAccount | 12 | 12 | 100.0 |
| CaseNumber | 24 | 0 | 0.0 |
| CompanyName | 6 | 6 | 100.0 |
| Date | 6 | 0 | 0.0 |
| DateOfBirth | 3 | 3 | 100.0 |
| MonetaryAmount | 6 | 0 | 0.0 |
| NationalId | 3 | 3 | 100.0 |
| PersonName | 156 | 138 | 88.5 |
| **all** | 225 | 168 | 74.7 |

### Layers 1+2+3 by category, run 1

| category | labels | L1+2 P / R | L3 P / R | L1+2+3 P / R |
|---|---:|---|---|---|
| Address | 29 | 100.0 / 89.7 | 90.9 / 65.5 | 96.6 / 96.6 |
| BankAccount | 15 | 100.0 / 73.3 | 100.0 / 46.7 | 100.0 / 100.0 |
| CaseNumber | 6 | 100.0 / 83.3 | 11.1 / 33.3 | 38.5 / 83.3 |
| ClientId | 5 | 100.0 / 80.0 | 100.0 / 0.0 | 100.0 / 80.0 |
| CompanyName | 37 | 92.0 / 81.1 | 100.0 / 73.0 | 92.6 / 94.6 |
| CourtFileNumber | 1 | 100.0 / 100.0 | – / 0.0 | 100.0 / 100.0 |
| CreditCard | 8 | 100.0 / 100.0 | – / 25.0 | 100.0 / 100.0 |
| Date | 72 | 100.0 / 91.7 | 85.7 / 27.8 | 97.1 / 91.7 |
| DateOfBirth | 9 | 100.0 / 88.9 | 100.0 / 55.6 | 100.0 / 100.0 |
| DriversLicense | 4 | 100.0 / 100.0 | 100.0 / 25.0 | 100.0 / 100.0 |
| Email | 25 | 100.0 / 96.0 | 100.0 / 40.0 | 100.0 / 96.0 |
| IBAN | 9 | 100.0 / 100.0 | 100.0 / 44.4 | 100.0 / 100.0 |
| IpAddress | 6 | 100.0 / 100.0 | – / 50.0 | 100.0 / 100.0 |
| LicensePlate | 5 | 100.0 / 100.0 | – / 0.0 | 100.0 / 100.0 |
| MedicalRecordNumber | 4 | 100.0 / 100.0 | – / 50.0 | 100.0 / 100.0 |
| MonetaryAmount | 46 | 100.0 / 100.0 | 88.2 / 28.3 | 95.8 / 100.0 |
| NationalId | 17 | 100.0 / 88.2 | 100.0 / 52.9 | 100.0 / 94.1 |
| PassportNumber | 3 | 100.0 / 100.0 | – / 66.7 | 100.0 / 100.0 |
| PersonName | 113 | 50.0 / 54.0 | 90.6 / 83.2 | 60.9 / 92.0 |
| PhoneNumber | 25 | 100.0 / 100.0 | 100.0 / 60.0 | 100.0 / 100.0 |
| SocialSecurityNumber | 3 | 100.0 / 100.0 | 100.0 / 66.7 | 100.0 / 100.0 |
| TaxId | 16 | 100.0 / 100.0 | – / 25.0 | 100.0 / 100.0 |
| Url | 3 | 100.0 / 100.0 | – / 66.7 | 100.0 / 100.0 |

### Layers 1+2+3 errors, run 1

False negatives (59):

- `en_court_complaint` p1 PersonName (partial): "Marcus T. Feeney"
- `en_court_complaint` p1 CompanyName (partial): "Marsh & Okonjo LLP"
- `en_demand_letter` p1 CompanyName (partial): "Okafor Whitfield PLLC"
- `en_demand_letter` p1 PersonName (partial): "Harriet Pemberton-Shaw"
- `en_demand_letter` p1 ClientId (partial): "NGM-24-0073318"
- `en_demand_letter` p1 MedicalRecordNumber (partial): "MRN 00482913"
- `en_demo_pdf` p1 Email (missed): "j.richardson@atlas-  \n  digital.com"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 Address (missed): "Westerdok 118, 1013 BH Amsterdam"
- `en_eu_share_purchase` p1 PersonName (missed): "Felix Brandauer"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine France SAS"
- `en_eu_share_purchase` p1 CaseNumber (missed): "16 O 248/24"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 PersonName (missed): "Joost van der Linde"
- `en_garnishment_affidavit` p1 PersonName (missed): "LORNA FETTERMAN"
- `en_settlement_agreement` p1 PersonName (partial): "S. Achterberg"
- `en_settlement_agreement` p1 PersonName (partial): "Ms Achterberg"
- `en_traps_llc_agreement` p1 PersonName (partial): "Constance Varga-Ellison"
- `es_carta_requerimiento` p1 CompanyName (partial): "Bufete Arriaga & Solís, S.C."
- `es_carta_requerimiento` p1 PersonName (missed): "Ramírez de la Garza"
- `es_contrato_arrendamiento` p1 PersonName (partial): "Tomás Ezequiel Barroso\nQuintanilla"
- `es_contrato_arrendamiento` p1 CompanyName (partial): "Inversiones Quintanilla Hermanos, S.L."
- `es_contrato_arrendamiento` p1 PersonName (partial): "Tomás E. Barroso Quintanilla"
- `es_demanda_reclamacion` p1 PersonName (missed): "LLORENTE ESCRIVÁ, Nuria"
- `es_demanda_reclamacion` p1 PersonName (missed): "Beatriz Coloma Ruano"
- `es_demanda_reclamacion` p1 CompanyName (partial): "Reformas Turia Integral, S.L.U."
- `holdout_en_intake_memo` p1 PersonName (partial): "Naomi Fairbanks-Oyelaran"
- `holdout_en_intake_memo` p1 Date (missed): "2025.03.18"
- `holdout_en_intake_memo` p1 PersonName (partial): "Desmond Achebe-Kowalczyk"
- `holdout_en_intake_memo` p1 CompanyName (missed): "Pellucid Freight Systems, Inc."
- `holdout_en_intake_memo` p1 ClientId (missed): "M-25-0331"
- `holdout_en_intake_memo` p1 PersonName (partial): "Desmond Achebe-Kowalczyk"
- `holdout_en_intake_memo` p1 Address (partial): "83 Lantern Hill Terrace, Unit 12, Marietta, GA 30064-3317"
- `holdout_en_intake_memo` p1 PersonName (missed): "Desmond"
- `holdout_en_intake_memo` p1 Date (missed): "Feb. 3, 2021"
- `holdout_en_intake_memo` p1 Date (missed): "Nov. 30, 2024"
- `holdout_en_intake_memo` p1 CompanyName (partial): "Quist Harrow & Lyle LLP"
- `holdout_es_acta_junta` p1 CompanyName (missed): "TALLERES METÁLICOS ARGAÑA, S.L."
- `holdout_es_acta_junta` p1 PersonName (partial): "Íñigo Zabaleta Urrutia"
- `holdout_es_acta_junta` p1 PersonName (missed): "Zabaleta"
- `holdout_es_acta_junta` p1 PersonName (missed): "Beloki"
- `holdout_es_acta_junta` p1 CompanyName (partial): "Auditores Ibaizabal, S.L.P."
- `holdout_es_acta_junta` p1 Address (partial): "Alameda de Recalde, 27, 5.º dcha., 48009 Bilbao"
- `holdout_es_acta_junta` p1 PhoneNumber (partial): "0034 944 555 120"
- `holdout_es_acta_junta` p1 PersonName (partial): "Íñigo Zabaleta Urrutia"
- `holdout_pt_notificacao` p1 NationalId (missed): "39053344705"
- `holdout_pt_notificacao` p1 Date (missed): "10 jan. 2025"
- `holdout_pt_notificacao` p1 Date (missed): "10 fev. 2025"
- `holdout_pt_notificacao` p1 Date (missed): "10 mar. 2025"
- `pt_contrato_prestacao` p1 PersonName (partial): "Helena Cristina dos Santos Albuquerque"
- `pt_contrato_prestacao` p1 PersonName (partial): "Priscila\nFagundes Teixeira"
- `pt_contrato_prestacao` p1 CompanyName (partial): "LATICÍNIOS SERRA AZUL LTDA."
- `pt_contrato_prestacao` p1 PersonName (partial): "Helena Cristina dos Santos Albuquerque"
- `pt_peticao_inicial` p1 PersonName (partial): "CARVALHO, Juliana Rezende de"
- `pt_peticao_inicial` p1 PersonName (partial): "Edvaldo\nNogueira Paixão"
- `pt_procuracao_portugal` p1 CompanyName (partial): "Douro Vinhos & Quintas, Lda."

False positives (87):

- `en_court_complaint` p1 PersonName [heuristic]: "See Burlington"
- `en_court_complaint` p1 PersonName [heuristic]: "Civil Rights Act"
- `en_court_complaint` p1 PersonName [llm]: "White"
- `en_court_complaint` p1 PersonName [llm]: "Yartzoff v. Thomas"
- `en_court_complaint` p1 PersonName [llm]: "Yartzoff"
- `en_demand_letter` p1 PersonName [heuristic]: "Claims Department"
- `en_demand_letter` p1 PersonName [heuristic]: "Claim No"
- `en_demand_letter` p1 PersonName [heuristic]: "Dear Claims Representative"
- `en_demand_letter` p1 PersonName [heuristic]: "New York"
- `en_demand_letter` p1 PersonName [heuristic]: "Because Mr"
- `en_demo_pdf` p1 PersonName [heuristic]: "Contract No"
- `en_demo_pdf` p1 PersonName [heuristic]: "The Project Manager"
- `en_demo_pdf` p1 PersonName [heuristic]: "The Client"
- `en_demo_pdf` p1 PersonName [llm]: "VP of Engineering"
- `en_demo_pdf` p2 PersonName [heuristic]: "Internal Revenue Service"
- `en_demo_pdf` p2 PersonName [heuristic]: "Confidential Information"
- `en_demo_pdf` p2 MonetaryAmount [llm]: "1.5%"
- `en_demo_pdf` p2 Date [llm]: "60"
- `en_demo_pdf` p3 PersonName [heuristic]: "San Francisco"
- `en_demo_pdf` p3 PersonName [heuristic]: "American Arbitration Association"
- `en_demo_pdf` p3 PersonName [heuristic]: "Managing Partner"
- `en_demo_pdf` p3 PersonName [heuristic]: "Chief Executive Officer"
- `en_demo_pdf` p3 PersonName [heuristic]: "Senior Director"
- `en_demo_pdf` p3 CaseNumber [llm]: "10"
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
- `en_traps_llc_agreement` p1 PersonName [llm]: "Internal Revenue Code"
- `es_carta_requerimiento` p1 PersonName [heuristic]: "Ciudad de México"
- `es_carta_requerimiento` p1 PersonName [heuristic]: "Estimada Sra"
- `es_contrato_arrendamiento` p1 PersonName [heuristic]: "En Madrid"
- `es_contrato_arrendamiento` p1 PersonName [heuristic]: "Arrendamientos Urbanos"
- `es_contrato_arrendamiento` p1 Address [llm]: "Agencia de Vivienda Social de la Comunidad de Madrid"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Juicio Verbal"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Ley de Enjuiciamiento Civil"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Código Civil"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "En Valencia"
- `es_demanda_reclamacion` p1 CaseNumber [llm]: "2024/0031"
- `es_demanda_reclamacion` p1 CaseNumber [llm]: "1.101"
- `es_demanda_reclamacion` p1 CaseNumber [llm]: "1.124"
- `es_demanda_reclamacion` p1 CaseNumber [llm]: "1.544"
- `es_demanda_reclamacion` p1 CaseNumber [llm]: "1/2015"
- `es_demanda_reclamacion` p1 CaseNumber [llm]: "27 de enero"
- `holdout_en_intake_memo` p1 PersonName [heuristic]: "Litigation Team"
- `holdout_es_acta_junta` p1 PersonName [heuristic]: "En Bilbao"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Prezado Sr"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Ofício de Registro de Imóveis de Belo Horizonte"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Lei do Inquilinato"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Belo Horizonte"
- `holdout_pt_notificacao` p1 MonetaryAmount [llm]: "10%"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "Banco do Brasil"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "Lei Geral de Proteção de Dados"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "Foro da Comarca de São Paulo"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "São Paulo"
- `pt_peticao_inicial` p1 PersonName [heuristic]: "Código de Processo Civil"
- `pt_peticao_inicial` p1 PersonName [heuristic]: "São Paulo"
- `pt_peticao_inicial` p1 CaseNumber [llm]: "1.737.412/SE"
- `pt_procuracao_portugal` p1 PersonName [heuristic]: "Caro Sr"
- `pt_procuracao_portugal` p1 Date [llm]: "2024"

### Layer 3 alone false positives, run 1

- `en_court_complaint` p1 PersonName: "White"
- `en_court_complaint` p1 PersonName: "Yartzoff v. Thomas"
- `en_court_complaint` p1 PersonName: "Yartzoff"
- `en_demo_pdf` p1 PersonName: "VP of Engineering"
- `en_demo_pdf` p2 MonetaryAmount: "1.5%"
- `en_demo_pdf` p2 Date: "60"
- `en_demo_pdf` p3 CaseNumber: "10"
- `en_settlement_agreement` p1 PersonName: "Achterberg"
- `en_traps_llc_agreement` p1 PersonName: "Force Majeure Event"
- `en_traps_llc_agreement` p1 PersonName: "Internal Revenue Code"
- `en_traps_llc_agreement` p1 PersonName: "Internal Revenue Code"
- `en_traps_llc_agreement` p1 PersonName: "Preferred Return"
- `en_traps_llc_agreement` p1 PersonName: "Tag-Along Notice"
- `en_traps_llc_agreement` p1 PersonName: "Managing Director"
- `es_contrato_arrendamiento` p1 Address: "Agencia de Vivienda Social de la Comunidad de Madrid"
- `es_demanda_reclamacion` p1 CaseNumber: "2024/0031"
- `es_demanda_reclamacion` p1 CaseNumber: "1.101"
- `es_demanda_reclamacion` p1 CaseNumber: "1.124"
- `es_demanda_reclamacion` p1 CaseNumber: "1.544"
- `es_demanda_reclamacion` p1 CaseNumber: "1/2015"
- `es_demanda_reclamacion` p1 CaseNumber: "27 de enero"
- `holdout_pt_notificacao` p1 Address: "Belo Horizonte"
- `holdout_pt_notificacao` p1 MonetaryAmount: "10%"
- `holdout_pt_notificacao` p1 Address: "Belo Horizonte"
- `pt_peticao_inicial` p1 CaseNumber: "1.737.412/SE"
- `pt_procuracao_portugal` p1 Date: "2024"
