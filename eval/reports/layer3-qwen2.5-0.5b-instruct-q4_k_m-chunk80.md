# Eval: Layer 3 with `qwen2.5-0.5b-instruct-q4_k_m`

assessFit: **likely-fits** · load 0.8 s · 18 pages · 3 run(s) · prompt `p3` · chunks of 80 words, 11 overlap · post-processing: default

## Overall (overlap matching; mean ± sd over runs)

| | P | R | F1 | covered R | EN F1 | ES F1 | PT F1 |
|---|---|---|---|---|---|---|---|
| Layer 3 alone | 76.7 ± 0.0 | 52.3 ± 0.0 | 62.2 ± 0.0 | 47.9 ± 0.0 | 59.3 ± 0.0 | 69.7 ± 0.0 | 63.6 ± 0.0 |
| Layers 1+2 | 84.9 | 83.1 | 84.0 | 75.5 | 80.0 | 88.2 | 90.2 |
| Layers 1+2+3 | 75.1 ± 0.0 | 86.6 ± 0.0 | 80.4 ± 0.0 | 78.7 ± 0.0 | 74.5 ± 0.0 | 88.5 ± 0.0 | 89.9 ± 0.0 |

Speed: 7.37 ± 0.60 s/page (Layer 3 only; 94 chunks/run, 0 failed chunks over all runs: 0 engine errors, 0 unparseable answers). Peak RSS of the sidecar: 942 MB.

Run-to-run: span-set Jaccard 100.0 ± 0.0 (pairs of runs), identical raw answers 100.0 ± 0.0 of chunks.

### Exact / overlap / covered, run 1

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| Layer 3 alone | 461 | 317 | 76.7 / 52.3 / 62.2 | 60.9 / 41.9 / 49.6 | 47.9 | 65.1 |
| Layers 1+2+3 | 461 | 530 | 75.1 / 86.6 / 80.4 | 66.4 / 76.4 / 71.0 | 78.7 | 96.2 |
| Layers 1+2+3 – dev | 401 | 479 | 73.9 / 88.5 / 80.6 | 67.0 / 80.0 / 73.0 | 82.3 | 96.3 |
| Layers 1+2+3 – holdout | 60 | 51 | 86.3 / 73.3 / 79.3 | 60.8 / 51.7 / 55.9 | 55.0 | 95.5 |

### Prompt versions (run 1 each)

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| `p3` Layer 3 alone (run 1) | 461 | 317 | 76.7 / 52.3 / 62.2 | 60.9 / 41.9 / 49.6 | 47.9 | 65.1 |
| `p3` Layers 1+2+3 (run 1) | 461 | 530 | 75.1 / 86.6 / 80.4 | 66.4 / 76.4 / 71.0 | 78.7 | 96.2 |

### Post-processing ablation (replayed on run 1's answers)

| config | L3 P | L3 R | L3 F1 | L1+2+3 P | L1+2+3 R | L1+2+3 F1 |
|---|---|---|---|---|---|---|
| default | 76.7 | 52.3 | 62.2 | 75.1 | 86.6 | 80.4 |
| Phase 1B post-processing | 79.9 | 47.9 | 59.9 | 77.0 | 85.2 | 80.9 |
| − stop-list | 70.8 | 52.3 | 60.2 | 71.8 | 86.6 | 78.5 |
| − Last, First variants | 76.6 | 52.1 | 62.0 | 75.0 | 86.3 | 80.3 |
| − honorific + surname | 76.4 | 51.6 | 61.6 | 75.0 | 85.9 | 80.1 |
| − page-wide mapping | 83.2 | 48.8 | 61.5 | 78.7 | 86.1 | 82.3 |
| − accent folding | 76.7 | 52.3 | 62.2 | 75.1 | 86.6 | 80.4 |
| − hyphen line-wrap | 76.7 | 52.3 | 62.2 | 75.1 | 86.6 | 80.4 |

### What happened to the model's items (run 1)

```
{
  "items": 1518,
  "salvaged_chunks": 38,
  "invalid_items": 0,
  "unknown_category": 0,
  "empty_text": 0,
  "implausible": 69,
  "stoplisted": 8,
  "not_in_chunk": 159,
  "accepted": 327,
  "variant_hits": 4,
  "page_wide_hits": 57,
  "covered_by_layers_1_2": 0
}
```

### Layer 3 alone by category (all runs pooled; for confidence calibration)

| category | preds | P (overlap) | R (overlap) | type acc. |
|---|---:|---:|---:|---:|
| Address | 252 | 81.0 | 96.6 | 100.0 |
| BankAccount | 3 | 100.0 | 13.3 | 50.0 |
| CaseNumber | 21 | 57.1 | 16.7 | 100.0 |
| ClientId | 15 | 100.0 | 80.0 | 75.0 |
| CompanyName | 207 | 44.9 | 75.7 | 82.1 |
| CourtFileNumber | 0 | – | 100.0 | 0.0 |
| CreditCard | 0 | – | 25.0 | 0.0 |
| Date | 69 | 65.2 | 45.8 | 42.4 |
| DateOfBirth | 45 | 93.3 | 33.3 | 66.7 |
| DriversLicense | 0 | – | 50.0 | 0.0 |
| Email | 24 | 100.0 | 36.0 | 88.9 |
| IBAN | 0 | – | 33.3 | 0.0 |
| IpAddress | 0 | – | 66.7 | 0.0 |
| LicensePlate | 0 | – | 20.0 | 0.0 |
| MedicalRecordNumber | 0 | – | 25.0 | 0.0 |
| MonetaryAmount | 111 | 83.8 | 71.7 | 93.9 |
| NationalId | 93 | 93.5 | 70.6 | 91.7 |
| PassportNumber | 0 | – | 66.7 | 0.0 |
| PersonName | 54 | 100.0 | 31.0 | 51.4 |
| PhoneNumber | 57 | 100.0 | 96.0 | 70.8 |
| SocialSecurityNumber | 0 | – | 66.7 | 0.0 |
| TaxId | 0 | – | 56.2 | 0.0 |
| Url | 0 | – | 66.7 | 0.0 |

Agreement (run 1): detections confirmed by two overlapping chunks: 34/38 correct (89.5); single-chunk: 209/279 (74.9).

### Layers 1+2+3 by category, run 1

| category | labels | L1+2 P / R | L3 P / R | L1+2+3 P / R |
|---|---:|---|---|---|
| Address | 29 | 100.0 / 89.7 | 81.0 / 96.6 | 74.4 / 100.0 |
| BankAccount | 15 | 100.0 / 73.3 | 100.0 / 13.3 | 100.0 / 73.3 |
| CaseNumber | 6 | 100.0 / 83.3 | 57.1 / 16.7 | 71.4 / 83.3 |
| ClientId | 5 | 100.0 / 80.0 | 100.0 / 80.0 | 100.0 / 80.0 |
| CompanyName | 37 | 92.0 / 81.1 | 44.9 / 75.7 | 41.9 / 86.5 |
| CourtFileNumber | 1 | 100.0 / 100.0 | – / 100.0 | 100.0 / 100.0 |
| CreditCard | 8 | 100.0 / 100.0 | – / 25.0 | 100.0 / 100.0 |
| Date | 72 | 100.0 / 91.7 | 65.2 / 45.8 | 89.3 / 93.1 |
| DateOfBirth | 9 | 100.0 / 88.9 | 93.3 / 33.3 | 88.9 / 88.9 |
| DriversLicense | 4 | 100.0 / 100.0 | – / 50.0 | 100.0 / 100.0 |
| Email | 25 | 100.0 / 96.0 | 100.0 / 36.0 | 100.0 / 96.0 |
| IBAN | 9 | 100.0 / 100.0 | – / 33.3 | 100.0 / 100.0 |
| IpAddress | 6 | 100.0 / 100.0 | – / 66.7 | 100.0 / 100.0 |
| LicensePlate | 5 | 100.0 / 100.0 | – / 20.0 | 100.0 / 100.0 |
| MedicalRecordNumber | 4 | 100.0 / 100.0 | – / 25.0 | 100.0 / 100.0 |
| MonetaryAmount | 46 | 100.0 / 100.0 | 83.8 / 71.7 | 88.5 / 100.0 |
| NationalId | 17 | 100.0 / 88.2 | 93.5 / 70.6 | 88.9 / 94.1 |
| PassportNumber | 3 | 100.0 / 100.0 | – / 66.7 | 100.0 / 100.0 |
| PersonName | 113 | 50.0 / 54.0 | 100.0 / 31.0 | 51.8 / 61.9 |
| PhoneNumber | 25 | 100.0 / 100.0 | 100.0 / 96.0 | 100.0 / 100.0 |
| SocialSecurityNumber | 3 | 100.0 / 100.0 | – / 66.7 | 100.0 / 100.0 |
| TaxId | 16 | 100.0 / 100.0 | – / 56.2 | 100.0 / 100.0 |
| Url | 3 | 100.0 / 100.0 | – / 66.7 | 100.0 / 100.0 |

### Layers 1+2+3 errors, run 1

False negatives (98):

- `en_court_complaint` p1 PersonName (partial): "Marcus T. Feeney"
- `en_court_complaint` p1 PersonName (missed): "Priya\nRamaswamy"
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
- `en_demand_letter` p1 PersonName (missed): "T. R. Lindqvist"
- `en_demo_pdf` p1 Email (missed): "j.richardson@atlas-  \n  digital.com"
- `en_demo_pdf` p1 PersonName (missed): "Roberto  \n  Fernandes"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 PersonName (missed): "Felix Brandauer"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine France SAS"
- `en_eu_share_purchase` p1 CaseNumber (missed): "16 O 248/24"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 PersonName (missed): "Joost van der Linde"
- `en_garnishment_affidavit` p1 CompanyName (missed): "Desert Sun Credit Union"
- `en_garnishment_affidavit` p1 PersonName (missed): "Calloway, Brent J."
- `en_garnishment_affidavit` p1 CompanyName (missed): "Desert Sun Credit Union"
- `en_garnishment_affidavit` p1 PersonName (missed): "Brent J. Calloway"
- `en_garnishment_affidavit` p1 BankAccount (missed): "0047-221983"
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
- `en_traps_llc_agreement` p1 PersonName (missed): "Imelda Szabo"
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
- `holdout_en_intake_memo` p1 Date (partial): "2025.03.18"
- `holdout_en_intake_memo` p1 PersonName (partial): "Desmond Achebe-Kowalczyk"
- `holdout_en_intake_memo` p1 CompanyName (missed): "Pellucid Freight Systems, Inc."
- `holdout_en_intake_memo` p1 ClientId (missed): "M-25-0331"
- `holdout_en_intake_memo` p1 PersonName (partial): "Desmond Achebe-Kowalczyk"
- `holdout_en_intake_memo` p1 DateOfBirth (missed): "07.11.1968"
- `holdout_en_intake_memo` p1 Address (partial): "83 Lantern Hill Terrace, Unit 12, Marietta, GA 30064-3317"
- `holdout_en_intake_memo` p1 NationalId (missed): "046 454 286"
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
- `holdout_pt_notificacao` p1 Date (missed): "10 jan. 2025"
- `holdout_pt_notificacao` p1 Date (missed): "10 fev. 2025"
- `holdout_pt_notificacao` p1 Date (missed): "10 mar. 2025"
- `pt_contrato_prestacao` p1 PersonName (partial): "Helena Cristina dos Santos Albuquerque"
- `pt_contrato_prestacao` p1 PersonName (partial): "Priscila\nFagundes Teixeira"
- `pt_contrato_prestacao` p1 CompanyName (partial): "LATICÍNIOS SERRA AZUL LTDA."
- `pt_contrato_prestacao` p1 PersonName (partial): "Helena Cristina dos Santos Albuquerque"
- `pt_contrato_prestacao` p1 CompanyName (missed): "MOURA DANTAS CONSULTORIA TRIBUTÁRIA EIRELI"
- `pt_peticao_inicial` p1 PersonName (partial): "CARVALHO, Juliana Rezende de"
- `pt_peticao_inicial` p1 PersonName (partial): "Edvaldo\nNogueira Paixão"
- `pt_peticao_inicial` p1 PersonName (missed): "Paixão"
- `pt_procuracao_portugal` p1 CompanyName (partial): "Douro Vinhos & Quintas, Lda."
- `pt_procuracao_portugal` p1 PersonName (missed): "Fonseca, Joaquim"
- `pt_procuracao_portugal` p1 PersonName (missed): "J. M. T. Fonseca"

False positives (132):

- `en_court_complaint` p1 PersonName [heuristic]: "See Burlington"
- `en_court_complaint` p1 PersonName [heuristic]: "Civil Rights Act"
- `en_court_complaint` p1 Address [llm]: "Contra Costa County"
- `en_demand_letter` p1 PersonName [heuristic]: "Claims Department"
- `en_demand_letter` p1 PersonName [heuristic]: "Claim No"
- `en_demand_letter` p1 PersonName [heuristic]: "Dear Claims Representative"
- `en_demand_letter` p1 PersonName [heuristic]: "New York"
- `en_demand_letter` p1 PersonName [heuristic]: "Because Mr"
- `en_demand_letter` p1 Address [llm]: "New York"
- `en_demand_letter` p1 Address [llm]: "New York"
- `en_demand_letter` p1 Address [llm]: "Kings County"
- `en_demo_pdf` p1 PersonName [heuristic]: "Contract No"
- `en_demo_pdf` p1 PersonName [heuristic]: "The Project Manager"
- `en_demo_pdf` p1 PersonName [heuristic]: "The Client"
- `en_demo_pdf` p2 PersonName [heuristic]: "Internal Revenue Service"
- `en_demo_pdf` p2 PersonName [heuristic]: "Confidential Information"
- `en_demo_pdf` p2 MonetaryAmount [llm]: "1.5% per month"
- `en_demo_pdf` p2 Date [llm]: "6. EXPENSE REIMBURSEMENT"
- `en_demo_pdf` p2 Address [llm]: "held by"
- `en_demo_pdf` p2 MonetaryAmount [llm]: "7. TAX INFORMATION AND COMPLIANCE"
- `en_demo_pdf` p2 Address [llm]: "written consent"
- `en_demo_pdf` p3 PersonName [heuristic]: "San Francisco"
- `en_demo_pdf` p3 PersonName [heuristic]: "American Arbitration Association"
- `en_demo_pdf` p3 PersonName [heuristic]: "Managing Partner"
- `en_demo_pdf` p3 PersonName [heuristic]: "Chief Executive Officer"
- `en_demo_pdf` p3 PersonName [heuristic]: "Senior Director"
- `en_eu_share_purchase` p1 PersonName [heuristic]: "Purchase Price"
- `en_eu_share_purchase` p1 PersonName [heuristic]: "Managing Director"
- `en_eu_share_purchase` p1 Date [llm]: "1.1"
- `en_eu_share_purchase` p1 Date [llm]: "1.2"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Judgment Creditor"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Judgment Debtor"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Recovery Manager"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Judgment Debtor"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "American Express"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Judgment Debtor"
- `en_garnishment_affidavit` p1 PersonName [heuristic]: "Notary Public"
- `en_garnishment_affidavit` p1 Address [llm]: "Attn: Legal Process"
- `en_garnishment_affidavit` p1 Address [llm]: "Phoenix, Arizona"
- `en_settlement_agreement` p1 PersonName [heuristic]: "This Confidential Settlement Agreement"
- `en_settlement_agreement` p1 PersonName [heuristic]: "The Employee"
- `en_settlement_agreement` p1 PersonName [heuristic]: "Data Incident"
- `en_settlement_agreement` p1 PersonName [heuristic]: "National Insurance"
- `en_settlement_agreement` p1 PersonName [heuristic]: "Confidential Information"
- `en_settlement_agreement` p1 PersonName [heuristic]: "Effective Date"
- `en_settlement_agreement` p1 PersonName [heuristic]: "Data Protection Officer"
- `en_settlement_agreement` p1 PersonName [heuristic]: "Neither Party"
- `en_settlement_agreement` p1 MonetaryAmount [llm]: "2.1"
- `en_settlement_agreement` p1 MonetaryAmount [llm]: "2.1"
- `en_settlement_agreement` p1 Date [llm]: "3.1"
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
- `en_traps_llc_agreement` p1 CompanyName [llm]: "ARTICLE"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Section"
- `en_traps_llc_agreement` p1 NationalId [llm]: "6 Del. C. § 18-101 et seq."
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Section"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Section"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "ARTICLE"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Board"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "board"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Company described in Article V"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Section"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "God"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Governmental Authority"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Code"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Internal Revenue Code"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Section"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "ARTICLE"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Section"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Schedule"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Board"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Section"
- `en_traps_llc_agreement` p1 MonetaryAmount [llm]: "10,000 Class A Units"
- `en_traps_llc_agreement` p1 MonetaryAmount [llm]: "2,500 Class B Units"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "ARTICLE"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Section"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Section"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Section"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Code"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Section"
- `en_traps_llc_agreement` p1 DateOfBirth [llm]: "2025"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "ARTICLE"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Section"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Board"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Section"
- `en_traps_llc_agreement` p1 NationalId [llm]: "965 A.2d 695"
- `en_traps_llc_agreement` p1 Date [llm]: "906 A.2d 27"
- `en_traps_llc_agreement` p1 Date [llm]: "2006"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "ARTICLE"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Section"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Section"
- `en_traps_llc_agreement` p1 Date [llm]: "2.4 of the Uniform Commercial Code"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Section"
- `en_traps_llc_agreement` p1 CompanyName [llm]: "Section"
- `en_traps_llc_agreement` p1 Address [llm]: "By: ______________________"
- `es_carta_requerimiento` p1 PersonName [heuristic]: "Ciudad de México"
- `es_carta_requerimiento` p1 PersonName [heuristic]: "Estimada Sra"
- `es_contrato_arrendamiento` p1 PersonName [heuristic]: "En Madrid"
- `es_contrato_arrendamiento` p1 PersonName [heuristic]: "Arrendamientos Urbanos"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Juicio Verbal"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Ley de Enjuiciamiento Civil"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Código Civil"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "En Valencia"
- `es_demanda_reclamacion` p1 CaseNumber [llm]: "5 DE VALENCIA"
- `es_demanda_reclamacion` p1 CaseNumber [llm]: "STS 1/2015"
- `holdout_en_intake_memo` p1 PersonName [heuristic]: "Litigation Team"
- `holdout_es_acta_junta` p1 PersonName [heuristic]: "En Bilbao"
- `holdout_es_acta_junta` p1 Date [llm]: "2023"
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
- `pt_procuracao_portugal` p1 Address [llm]: "Rua de Santa Catarina"
- `pt_procuracao_portugal` p1 Address [llm]: "Cartório Notarial de Lisboa"

### Layer 3 alone false positives, run 1

- `en_court_complaint` p1 Address: "Contra Costa County"
- `en_court_complaint` p1 CaseNumber: "VII of the Civil Rights Act of 1964"
- `en_demand_letter` p1 Address: "New York"
- `en_demand_letter` p1 Address: "New York"
- `en_demand_letter` p1 Address: "New York"
- `en_demand_letter` p1 Address: "Kings County"
- `en_demo_pdf` p2 MonetaryAmount: "1.5% per month"
- `en_demo_pdf` p2 Date: "6. EXPENSE REIMBURSEMENT"
- `en_demo_pdf` p2 Address: "held by"
- `en_demo_pdf` p2 MonetaryAmount: "7. TAX INFORMATION AND COMPLIANCE"
- `en_demo_pdf` p2 Address: "written consent"
- `en_demo_pdf` p3 Address: "San Francisco, California"
- `en_eu_share_purchase` p1 Date: "1.1"
- `en_eu_share_purchase` p1 Date: "1.2"
- `en_garnishment_affidavit` p1 Address: "Attn: Legal Process"
- `en_garnishment_affidavit` p1 Address: "Phoenix, Arizona"
- `en_settlement_agreement` p1 MonetaryAmount: "2.1"
- `en_settlement_agreement` p1 MonetaryAmount: "2.1"
- `en_settlement_agreement` p1 Date: "3.1"
- `en_traps_llc_agreement` p1 CompanyName: "ARTICLE"
- `en_traps_llc_agreement` p1 CompanyName: "Section"
- `en_traps_llc_agreement` p1 NationalId: "6 Del. C. § 18-101 et seq."
- `en_traps_llc_agreement` p1 CompanyName: "Section"
- `en_traps_llc_agreement` p1 CompanyName: "Section"
- `en_traps_llc_agreement` p1 CompanyName: "ARTICLE"
- `en_traps_llc_agreement` p1 CompanyName: "Board"
- `en_traps_llc_agreement` p1 CompanyName: "board"
- `en_traps_llc_agreement` p1 CompanyName: "Company described in Article V"
- `en_traps_llc_agreement` p1 CompanyName: "Section"
- `en_traps_llc_agreement` p1 CompanyName: "God"
- `en_traps_llc_agreement` p1 CompanyName: "Governmental Authority"
- `en_traps_llc_agreement` p1 CompanyName: "Internal Revenue Code"
- `en_traps_llc_agreement` p1 CompanyName: "Code"
- `en_traps_llc_agreement` p1 CompanyName: "Internal Revenue Code"
- `en_traps_llc_agreement` p1 CompanyName: "Preferred Return"
- `en_traps_llc_agreement` p1 CompanyName: "Tag-Along Notice"
- `en_traps_llc_agreement` p1 CompanyName: "Section"
- `en_traps_llc_agreement` p1 CompanyName: "ARTICLE"
- `en_traps_llc_agreement` p1 CompanyName: "Section"
- `en_traps_llc_agreement` p1 CompanyName: "Schedule"
- `en_traps_llc_agreement` p1 CompanyName: "Board"
- `en_traps_llc_agreement` p1 CompanyName: "Section"
- `en_traps_llc_agreement` p1 MonetaryAmount: "10,000 Class A Units"
- `en_traps_llc_agreement` p1 MonetaryAmount: "2,500 Class B Units"
- `en_traps_llc_agreement` p1 CompanyName: "ARTICLE"
- `en_traps_llc_agreement` p1 CompanyName: "Section"
- `en_traps_llc_agreement` p1 CompanyName: "Section"
- `en_traps_llc_agreement` p1 CompanyName: "Section"
- `en_traps_llc_agreement` p1 CompanyName: "Code"
- `en_traps_llc_agreement` p1 CompanyName: "Section"
- `en_traps_llc_agreement` p1 DateOfBirth: "2025"
- `en_traps_llc_agreement` p1 CompanyName: "ARTICLE"
- `en_traps_llc_agreement` p1 CompanyName: "Section"
- `en_traps_llc_agreement` p1 CompanyName: "Board"
- `en_traps_llc_agreement` p1 CompanyName: "Board"
- `en_traps_llc_agreement` p1 CompanyName: "Section"
- `en_traps_llc_agreement` p1 NationalId: "965 A.2d 695"
- `en_traps_llc_agreement` p1 Date: "906 A.2d 27"
- `en_traps_llc_agreement` p1 Date: "2006"
- `en_traps_llc_agreement` p1 CompanyName: "ARTICLE"
- `en_traps_llc_agreement` p1 CompanyName: "Section"
- `en_traps_llc_agreement` p1 CompanyName: "Section"
- `en_traps_llc_agreement` p1 Date: "2.4 of the Uniform Commercial Code"
- `en_traps_llc_agreement` p1 CompanyName: "Section"
- `en_traps_llc_agreement` p1 CompanyName: "Section"
- `en_traps_llc_agreement` p1 Address: "By: ______________________"
- `en_traps_llc_agreement` p1 Address: "Title: Managing Director"
- `es_demanda_reclamacion` p1 CaseNumber: "5 DE VALENCIA"
- `es_demanda_reclamacion` p1 CaseNumber: "STS 1/2015"
- `holdout_es_acta_junta` p1 Date: "2023"
- `pt_contrato_prestacao` p1 Address: "São Paulo"
- `pt_contrato_prestacao` p1 Address: "São Paulo"
- `pt_procuracao_portugal` p1 Address: "Rua de Santa Catarina"
- `pt_procuracao_portugal` p1 Address: "Cartório Notarial de Lisboa"
