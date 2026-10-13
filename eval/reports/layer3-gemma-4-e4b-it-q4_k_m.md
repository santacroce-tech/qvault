# Eval: Layer 3 with `gemma-4-e4b-it-q4_k_m`

assessFit: **likely-fits** · load 3.5 s · 18 pages · 3 run(s) · prompt `p3` · chunks of 150 words, 20 overlap · post-processing: default

## Overall (overlap matching; mean ± sd over runs)

| | P | R | F1 | covered R | EN F1 | ES F1 | PT F1 |
|---|---|---|---|---|---|---|---|
| Layer 3 alone | 99.5 ± 0.0 | 89.2 ± 0.0 | 94.1 ± 0.0 | 88.7 ± 0.0 | 94.0 ± 0.0 | 95.7 ± 0.0 | 92.6 ± 0.0 |
| Layers 1+2 | 84.9 | 83.1 | 84.0 | 75.5 | 80.0 | 88.2 | 90.2 |
| Layers 1+2+3 | 86.4 ± 0.0 | 96.7 ± 0.0 | 91.3 ± 0.0 | 88.7 ± 0.0 | 89.0 ± 0.0 | 93.8 ± 0.0 | 94.8 ± 0.0 |

Speed: 15.45 ± 0.04 s/page (Layer 3 only; 53 chunks/run, 0 failed chunks over all runs: 0 engine errors, 0 unparseable answers). Peak RSS of the sidecar: 5751 MB.

Run-to-run: span-set Jaccard 100.0 ± 0.0 (pairs of runs), identical raw answers 100.0 ± 0.0 of chunks.

### Exact / overlap / covered, run 1

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| Layer 3 alone | 461 | 413 | 99.5 / 89.2 / 94.1 | 96.9 / 86.8 / 91.5 | 88.7 | 96.8 |
| Layers 1+2+3 | 461 | 514 | 86.4 / 96.7 / 91.3 | 77.6 / 86.6 / 81.8 | 88.7 | 97.3 |
| Layers 1+2+3 – dev | 401 | 456 | 86.0 / 98.0 / 91.6 | 78.3 / 89.0 / 83.3 | 91.3 | 97.7 |
| Layers 1+2+3 – holdout | 60 | 58 | 89.7 / 88.3 / 89.0 | 72.4 / 70.0 / 71.2 | 71.7 | 94.3 |

### Prompt versions (run 1 each)

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| `p3` Layer 3 alone (run 1) | 461 | 413 | 99.5 / 89.2 / 94.1 | 96.9 / 86.8 / 91.5 | 88.7 | 96.8 |
| `p3` Layers 1+2+3 (run 1) | 461 | 514 | 86.4 / 96.7 / 91.3 | 77.6 / 86.6 / 81.8 | 88.7 | 97.3 |

### Post-processing ablation (replayed on run 1's answers)

| config | L3 P | L3 R | L3 F1 | L1+2+3 P | L1+2+3 R | L1+2+3 F1 |
|---|---|---|---|---|---|---|
| default | 99.5 | 89.2 | 94.1 | 86.4 | 96.7 | 91.3 |
| Phase 1B post-processing | 99.5 | 84.8 | 91.6 | 86.0 | 93.5 | 89.6 |
| − stop-list | 99.5 | 89.2 | 94.1 | 86.4 | 96.7 | 91.3 |
| − Last, First variants | 99.5 | 88.3 | 93.6 | 86.3 | 95.9 | 90.8 |
| − honorific + surname | 99.5 | 87.9 | 93.3 | 86.2 | 95.4 | 90.6 |
| − page-wide mapping | 99.5 | 87.6 | 93.2 | 86.3 | 96.3 | 91.0 |
| − accent folding | 99.5 | 89.2 | 94.1 | 86.4 | 96.7 | 91.3 |
| − hyphen line-wrap | 99.5 | 88.9 | 93.9 | 86.4 | 96.5 | 91.2 |

### What happened to the model's items (run 1)

```
{
  "items": 418,
  "salvaged_chunks": 0,
  "invalid_items": 0,
  "unknown_category": 0,
  "empty_text": 0,
  "implausible": 1,
  "stoplisted": 0,
  "not_in_chunk": 1,
  "accepted": 414,
  "variant_hits": 16,
  "page_wide_hits": 41,
  "covered_by_layers_1_2": 0
}
```

### Layer 3 alone by category (all runs pooled; for confidence calibration)

| category | preds | P (overlap) | R (overlap) | type acc. |
|---|---:|---:|---:|---:|
| Address | 87 | 100.0 | 100.0 | 100.0 |
| BankAccount | 45 | 100.0 | 53.3 | 100.0 |
| CaseNumber | 12 | 100.0 | 33.3 | 100.0 |
| ClientId | 12 | 100.0 | 80.0 | 75.0 |
| CompanyName | 102 | 97.1 | 89.2 | 100.0 |
| CourtFileNumber | 0 | – | 100.0 | 0.0 |
| CreditCard | 24 | 100.0 | 100.0 | 100.0 |
| Date | 189 | 98.4 | 86.1 | 100.0 |
| DateOfBirth | 27 | 100.0 | 100.0 | 100.0 |
| DriversLicense | 12 | 100.0 | 100.0 | 100.0 |
| Email | 75 | 100.0 | 100.0 | 100.0 |
| IBAN | 6 | 100.0 | 100.0 | 22.2 |
| IpAddress | 18 | 100.0 | 100.0 | 100.0 |
| LicensePlate | 12 | 100.0 | 80.0 | 100.0 |
| MedicalRecordNumber | 9 | 100.0 | 100.0 | 75.0 |
| MonetaryAmount | 123 | 100.0 | 89.1 | 100.0 |
| NationalId | 54 | 100.0 | 100.0 | 94.1 |
| PassportNumber | 9 | 100.0 | 100.0 | 100.0 |
| PersonName | 300 | 100.0 | 88.5 | 100.0 |
| PhoneNumber | 75 | 100.0 | 100.0 | 100.0 |
| SocialSecurityNumber | 12 | 100.0 | 100.0 | 100.0 |
| TaxId | 27 | 100.0 | 68.8 | 81.8 |
| Url | 9 | 100.0 | 100.0 | 100.0 |

Agreement (run 1): detections confirmed by two overlapping chunks: 94/94 correct (100.0); single-chunk: 317/319 (99.4).

### Layer 3 additions to Layers 1+2 by category (all runs pooled; confidence calibration)

| category | added | correct | P |
|---|---:|---:|---:|
| Address | 9 | 9 | 100.0 |
| BankAccount | 12 | 12 | 100.0 |
| CompanyName | 24 | 21 | 87.5 |
| Date | 12 | 9 | 75.0 |
| DateOfBirth | 3 | 3 | 100.0 |
| Email | 3 | 3 | 100.0 |
| NationalId | 3 | 3 | 100.0 |
| PersonName | 126 | 126 | 100.0 |
| SocialSecurityNumber | 3 | 3 | 100.0 |
| **all** | 195 | 189 | 96.9 |

### Layers 1+2+3 by category, run 1

| category | labels | L1+2 P / R | L3 P / R | L1+2+3 P / R |
|---|---:|---|---|---|
| Address | 29 | 100.0 / 89.7 | 100.0 / 100.0 | 100.0 / 100.0 |
| BankAccount | 15 | 100.0 / 73.3 | 100.0 / 53.3 | 100.0 / 100.0 |
| CaseNumber | 6 | 100.0 / 83.3 | 100.0 / 33.3 | 100.0 / 83.3 |
| ClientId | 5 | 100.0 / 80.0 | 100.0 / 80.0 | 100.0 / 80.0 |
| CompanyName | 37 | 92.0 / 81.1 | 97.1 / 89.2 | 90.9 / 100.0 |
| CourtFileNumber | 1 | 100.0 / 100.0 | – / 100.0 | 100.0 / 100.0 |
| CreditCard | 8 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| Date | 72 | 100.0 / 91.7 | 98.4 / 86.1 | 98.6 / 95.8 |
| DateOfBirth | 9 | 100.0 / 88.9 | 100.0 / 100.0 | 100.0 / 100.0 |
| DriversLicense | 4 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| Email | 25 | 100.0 / 96.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| IBAN | 9 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| IpAddress | 6 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| LicensePlate | 5 | 100.0 / 100.0 | 100.0 / 80.0 | 100.0 / 100.0 |
| MedicalRecordNumber | 4 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| MonetaryAmount | 46 | 100.0 / 100.0 | 100.0 / 89.1 | 100.0 / 100.0 |
| NationalId | 17 | 100.0 / 88.2 | 100.0 / 100.0 | 100.0 / 100.0 |
| PassportNumber | 3 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| PersonName | 113 | 50.0 / 54.0 | 100.0 / 88.5 | 62.1 / 91.2 |
| PhoneNumber | 25 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| SocialSecurityNumber | 3 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| TaxId | 16 | 100.0 / 100.0 | 100.0 / 68.8 | 100.0 / 100.0 |
| Url | 3 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |

### Layers 1+2+3 errors, run 1

False negatives (52):

- `en_court_complaint` p1 PersonName (partial): "Marcus T. Feeney"
- `en_court_complaint` p1 CompanyName (partial): "Marsh & Okonjo LLP"
- `en_demand_letter` p1 CompanyName (partial): "Okafor Whitfield PLLC"
- `en_demand_letter` p1 PersonName (partial): "Harriet Pemberton-Shaw"
- `en_demand_letter` p1 ClientId (partial): "NGM-24-0073318"
- `en_demand_letter` p1 MedicalRecordNumber (partial): "MRN 00482913"
- `en_demand_letter` p1 PersonName (partial): "T. R. Lindqvist"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 PersonName (missed): "Felix Brandauer"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine France SAS"
- `en_eu_share_purchase` p1 CaseNumber (missed): "16 O 248/24"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_settlement_agreement` p1 PersonName (missed): "S. Achterberg"
- `en_settlement_agreement` p1 PersonName (missed): "Achterberg"
- `en_traps_llc_agreement` p1 PersonName (missed): "Constance Varga-Ellison"
- `en_traps_llc_agreement` p1 PersonName (missed): "Rupert Ashdown"
- `en_traps_llc_agreement` p1 PersonName (missed): "Imelda Szabo"
- `en_traps_llc_agreement` p1 PersonName (partial): "Constance Varga-Ellison"
- `es_carta_requerimiento` p1 CompanyName (partial): "Bufete Arriaga & Solís, S.C."
- `es_carta_requerimiento` p1 PersonName (missed): "Ramírez de la Garza"
- `es_contrato_arrendamiento` p1 PersonName (partial): "Tomás Ezequiel Barroso\nQuintanilla"
- `es_contrato_arrendamiento` p1 CompanyName (partial): "Inversiones Quintanilla Hermanos, S.L."
- `es_contrato_arrendamiento` p1 PersonName (partial): "Tomás E. Barroso Quintanilla"
- `es_demanda_reclamacion` p1 PersonName (partial): "LLORENTE ESCRIVÁ, Nuria"
- `es_demanda_reclamacion` p1 CompanyName (partial): "Reformas Turia Integral, S.L.U."
- `holdout_en_intake_memo` p1 PersonName (partial): "Naomi Fairbanks-Oyelaran"
- `holdout_en_intake_memo` p1 Date (missed): "2025.03.18"
- `holdout_en_intake_memo` p1 PersonName (partial): "Desmond Achebe-Kowalczyk"
- `holdout_en_intake_memo` p1 ClientId (missed): "M-25-0331"
- `holdout_en_intake_memo` p1 PersonName (partial): "Desmond Achebe-Kowalczyk"
- `holdout_en_intake_memo` p1 Address (partial): "83 Lantern Hill Terrace, Unit 12, Marietta, GA 30064-3317"
- `holdout_en_intake_memo` p1 PersonName (missed): "Desmond"
- `holdout_en_intake_memo` p1 Date (missed): "Feb. 3, 2021"
- `holdout_en_intake_memo` p1 Date (missed): "Nov. 30, 2024"
- `holdout_en_intake_memo` p1 CompanyName (partial): "Quist Harrow & Lyle LLP"
- `holdout_es_acta_junta` p1 PersonName (partial): "Íñigo Zabaleta Urrutia"
- `holdout_es_acta_junta` p1 PersonName (missed): "Zabaleta"
- `holdout_es_acta_junta` p1 PersonName (missed): "Beloki"
- `holdout_es_acta_junta` p1 CompanyName (partial): "Auditores Ibaizabal, S.L.P."
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

False positives (70):

- `en_court_complaint` p1 PersonName [heuristic]: "See Burlington"
- `en_court_complaint` p1 PersonName [heuristic]: "Civil Rights Act"
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
- `en_eu_share_purchase` p1 CompanyName [llm]: "Belgian affiliate"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Judgment Creditor"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Judgment Debtor"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Recovery Manager"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Judgment Debtor"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "American Express"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Judgment Debtor"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Notary Public"
- `en_garnishment_affidavit` p1 Date [llm]: "09/27"
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
- `pt_procuracao_portugal` p1 PersonName [heuristic]: "Caro Sr"

### Layer 3 alone false positives, run 1

- `en_eu_share_purchase` p1 CompanyName: "Belgian affiliate"
- `en_garnishment_affidavit` p1 Date: "09/27"
