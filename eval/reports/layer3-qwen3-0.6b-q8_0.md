# Eval: Layer 3 with `qwen3-0.6b-q8_0`

assessFit: **likely-fits** · load 0.9 s · 18 pages · 3 run(s) · prompt `p3` · chunks of 150 words, 20 overlap · post-processing: default

## Overall (overlap matching; mean ± sd over runs)

| | P | R | F1 | covered R | EN F1 | ES F1 | PT F1 |
|---|---|---|---|---|---|---|---|
| Layer 3 alone | 92.0 ± 0.0 | 49.9 ± 0.0 | 64.7 ± 0.0 | 49.0 ± 0.0 | 64.7 ± 0.0 | 58.6 ± 0.0 | 69.9 ± 0.0 |
| Layers 1+2 | 84.9 | 83.1 | 84.0 | 75.5 | 80.0 | 88.2 | 90.2 |
| Layers 1+2+3 | 82.7 ± 0.0 | 90.5 ± 0.0 | 86.4 ± 0.0 | 82.6 ± 0.0 | 84.7 ± 0.0 | 86.8 ± 0.0 | 90.3 ± 0.0 |

Speed: 3.70 ± 0.03 s/page (Layer 3 only; 53 chunks/run, 0 failed chunks over all runs: 0 engine errors, 0 unparseable answers). Peak RSS of the sidecar: 1302 MB.

Run-to-run: span-set Jaccard 100.0 ± 0.0 (pairs of runs), identical raw answers 100.0 ± 0.0 of chunks.

### Exact / overlap / covered, run 1

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| Layer 3 alone | 461 | 251 | 92.0 / 49.9 / 64.7 | 75.7 / 41.2 / 53.4 | 49.0 | 81.3 |
| Layers 1+2+3 | 461 | 502 | 82.7 / 90.5 / 86.4 | 73.5 / 80.0 / 76.6 | 82.6 | 96.9 |
| Layers 1+2+3 – dev | 401 | 445 | 82.7 / 92.0 / 87.1 | 74.6 / 82.8 / 78.5 | 85.5 | 97.0 |
| Layers 1+2+3 – holdout | 60 | 57 | 82.5 / 80.0 / 81.2 | 64.9 / 61.7 / 63.2 | 63.3 | 95.8 |

### Prompt versions (run 1 each)

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| `p3` Layer 3 alone (run 1) | 461 | 251 | 92.0 / 49.9 / 64.7 | 75.7 / 41.2 / 53.4 | 49.0 | 81.3 |
| `p3` Layers 1+2+3 (run 1) | 461 | 502 | 82.7 / 90.5 / 86.4 | 73.5 / 80.0 / 76.6 | 82.6 | 96.9 |

### Post-processing ablation (replayed on run 1's answers)

| config | L3 P | L3 R | L3 F1 | L1+2+3 P | L1+2+3 R | L1+2+3 F1 |
|---|---|---|---|---|---|---|
| default | 92.0 | 49.9 | 64.7 | 82.7 | 90.5 | 86.4 |
| Phase 1B post-processing | 88.8 | 46.2 | 60.8 | 81.1 | 87.9 | 84.3 |
| − stop-list | 85.2 | 49.9 | 62.9 | 79.5 | 90.5 | 84.6 |
| − Last, First variants | 91.9 | 48.8 | 63.7 | 82.5 | 89.4 | 85.8 |
| − honorific + surname | 91.9 | 49.0 | 63.9 | 82.6 | 89.8 | 86.0 |
| − page-wide mapping | 91.7 | 47.5 | 62.6 | 82.5 | 89.2 | 85.7 |
| − accent folding | 92.0 | 49.9 | 64.7 | 82.7 | 90.5 | 86.4 |
| − hyphen line-wrap | 92.0 | 49.9 | 64.7 | 82.7 | 90.5 | 86.4 |

### What happened to the model's items (run 1)

```
{
  "items": 535,
  "salvaged_chunks": 5,
  "invalid_items": 0,
  "unknown_category": 0,
  "empty_text": 0,
  "implausible": 66,
  "stoplisted": 4,
  "not_in_chunk": 68,
  "accepted": 250,
  "variant_hits": 12,
  "page_wide_hits": 21,
  "covered_by_layers_1_2": 0
}
```

### Layer 3 alone by category (all runs pooled; for confidence calibration)

| category | preds | P (overlap) | R (overlap) | type acc. |
|---|---:|---:|---:|---:|
| Address | 75 | 80.0 | 48.3 | 100.0 |
| BankAccount | 39 | 100.0 | 46.7 | 85.7 |
| CaseNumber | 54 | 77.8 | 50.0 | 100.0 |
| ClientId | 12 | 75.0 | 60.0 | 0.0 |
| CompanyName | 60 | 90.0 | 56.8 | 71.4 |
| CourtFileNumber | 0 | – | 100.0 | 0.0 |
| CreditCard | 3 | 0.0 | 12.5 | 0.0 |
| Date | 75 | 88.0 | 38.9 | 78.6 |
| DateOfBirth | 18 | 100.0 | 66.7 | 100.0 |
| DriversLicense | 0 | – | 75.0 | 0.0 |
| Email | 36 | 100.0 | 48.0 | 100.0 |
| IBAN | 3 | 100.0 | 77.8 | 14.3 |
| IpAddress | 0 | – | 0.0 | 100.0 |
| LicensePlate | 6 | 50.0 | 60.0 | 0.0 |
| MedicalRecordNumber | 0 | – | 0.0 | 100.0 |
| MonetaryAmount | 87 | 96.6 | 65.2 | 90.0 |
| NationalId | 42 | 100.0 | 64.7 | 100.0 |
| PassportNumber | 0 | – | 66.7 | 0.0 |
| PersonName | 183 | 98.4 | 48.7 | 96.4 |
| PhoneNumber | 42 | 100.0 | 64.0 | 81.2 |
| SocialSecurityNumber | 12 | 75.0 | 66.7 | 100.0 |
| TaxId | 6 | 100.0 | 31.2 | 40.0 |
| Url | 0 | – | 0.0 | 100.0 |

Agreement (run 1): detections confirmed by two overlapping chunks: 27/28 correct (96.4); single-chunk: 204/223 (91.5).

### Layer 3 additions to Layers 1+2 by category (all runs pooled; confidence calibration)

| category | added | correct | P |
|---|---:|---:|---:|
| Address | 12 | 0 | 0.0 |
| BankAccount | 12 | 12 | 100.0 |
| CaseNumber | 12 | 0 | 0.0 |
| ClientId | 3 | 0 | 0.0 |
| CompanyName | 15 | 9 | 60.0 |
| CreditCard | 3 | 0 | 0.0 |
| Date | 18 | 9 | 50.0 |
| DateOfBirth | 3 | 3 | 100.0 |
| LicensePlate | 3 | 0 | 0.0 |
| MonetaryAmount | 3 | 0 | 0.0 |
| NationalId | 3 | 3 | 100.0 |
| PersonName | 69 | 66 | 95.7 |
| SocialSecurityNumber | 3 | 0 | 0.0 |
| **all** | 159 | 102 | 64.2 |

### Layers 1+2+3 by category, run 1

| category | labels | L1+2 P / R | L3 P / R | L1+2+3 P / R |
|---|---:|---|---|---|
| Address | 29 | 100.0 / 89.7 | 80.0 / 48.3 | 86.7 / 89.7 |
| BankAccount | 15 | 100.0 / 73.3 | 100.0 / 46.7 | 100.0 / 100.0 |
| CaseNumber | 6 | 100.0 / 83.3 | 77.8 / 50.0 | 55.6 / 83.3 |
| ClientId | 5 | 100.0 / 80.0 | 75.0 / 60.0 | 75.0 / 80.0 |
| CompanyName | 37 | 92.0 / 81.1 | 90.0 / 56.8 | 86.7 / 94.6 |
| CourtFileNumber | 1 | 100.0 / 100.0 | – / 100.0 | 100.0 / 100.0 |
| CreditCard | 8 | 100.0 / 100.0 | 0.0 / 12.5 | 88.9 / 100.0 |
| Date | 72 | 100.0 / 91.7 | 88.0 / 38.9 | 95.8 / 95.8 |
| DateOfBirth | 9 | 100.0 / 88.9 | 100.0 / 66.7 | 100.0 / 100.0 |
| DriversLicense | 4 | 100.0 / 100.0 | – / 75.0 | 100.0 / 100.0 |
| Email | 25 | 100.0 / 96.0 | 100.0 / 48.0 | 100.0 / 96.0 |
| IBAN | 9 | 100.0 / 100.0 | 100.0 / 77.8 | 100.0 / 100.0 |
| IpAddress | 6 | 100.0 / 100.0 | – / 0.0 | 100.0 / 100.0 |
| LicensePlate | 5 | 100.0 / 100.0 | 50.0 / 60.0 | 83.3 / 100.0 |
| MedicalRecordNumber | 4 | 100.0 / 100.0 | – / 0.0 | 100.0 / 100.0 |
| MonetaryAmount | 46 | 100.0 / 100.0 | 96.6 / 65.2 | 97.9 / 100.0 |
| NationalId | 17 | 100.0 / 88.2 | 100.0 / 64.7 | 100.0 / 94.1 |
| PassportNumber | 3 | 100.0 / 100.0 | – / 66.7 | 100.0 / 100.0 |
| PersonName | 113 | 50.0 / 54.0 | 98.4 / 48.7 | 56.8 / 71.7 |
| PhoneNumber | 25 | 100.0 / 100.0 | 100.0 / 64.0 | 100.0 / 100.0 |
| SocialSecurityNumber | 3 | 100.0 / 100.0 | 75.0 / 66.7 | 75.0 / 100.0 |
| TaxId | 16 | 100.0 / 100.0 | 100.0 / 31.2 | 100.0 / 100.0 |
| Url | 3 | 100.0 / 100.0 | – / 0.0 | 100.0 / 100.0 |

### Layers 1+2+3 errors, run 1

False negatives (80):

- `en_court_complaint` p1 PersonName (missed): "HOLLOWAY, Dana"
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
- `en_demand_letter` p1 MedicalRecordNumber (partial): "MRN 00482913"
- `en_demand_letter` p1 PersonName (missed): "A. N. Castellanos"
- `en_demand_letter` p1 PersonName (missed): "Lindqvist"
- `en_demand_letter` p1 PersonName (missed): "J. Whitfield"
- `en_demand_letter` p1 PersonName (missed): "T. R. Lindqvist"
- `en_demo_pdf` p1 Email (missed): "j.richardson@atlas-  \n  digital.com"
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
- `en_eu_share_purchase` p1 PersonName (missed): "Joost van der Linde"
- `en_garnishment_affidavit` p1 PersonName (missed): "LORNA FETTERMAN"
- `en_settlement_agreement` p1 PersonName (missed): "S. Achterberg"
- `en_settlement_agreement` p1 PersonName (missed): "Achterberg"
- `en_settlement_agreement` p1 PersonName (missed): "Rafferty\nOkonkwo-Bell"
- `en_settlement_agreement` p1 PersonName (partial): "Ms Achterberg"
- `en_settlement_agreement` p1 CompanyName (missed): "HALCYON DATA SYSTEMS LTD"
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
- `es_demanda_reclamacion` p1 PersonName (missed): "Beatriz Coloma Ruano"
- `es_demanda_reclamacion` p1 CompanyName (partial): "Reformas Turia Integral, S.L.U."
- `es_demanda_reclamacion` p1 PersonName (missed): "Fermín Aleixandre Bou"
- `es_demanda_reclamacion` p1 PersonName (missed): "Aleixandre"
- `es_demanda_reclamacion` p1 PersonName (missed): "Álvaro Sendra Peiró"
- `holdout_en_intake_memo` p1 PersonName (partial): "Naomi Fairbanks-Oyelaran"
- `holdout_en_intake_memo` p1 Date (missed): "2025.03.18"
- `holdout_en_intake_memo` p1 PersonName (partial): "Desmond Achebe-Kowalczyk"
- `holdout_en_intake_memo` p1 ClientId (missed): "M-25-0331"
- `holdout_en_intake_memo` p1 PersonName (partial): "Desmond Achebe-Kowalczyk"
- `holdout_en_intake_memo` p1 Address (partial): "83 Lantern Hill Terrace, Unit 12, Marietta, GA 30064-3317"
- `holdout_en_intake_memo` p1 Address (missed): "2210 Rue Sainte-Catherine Ouest, Montréal, QC H3H 1M7"
- `holdout_en_intake_memo` p1 PersonName (missed): "Desmond"
- `holdout_en_intake_memo` p1 Date (missed): "Feb. 3, 2021"
- `holdout_en_intake_memo` p1 Date (missed): "Nov. 30, 2024"
- `holdout_en_intake_memo` p1 CompanyName (partial): "Quist Harrow & Lyle LLP"
- `holdout_es_acta_junta` p1 CompanyName (missed): "TALLERES METÁLICOS ARGAÑA, S.L."
- `holdout_es_acta_junta` p1 Address (missed): "C/ Gran Vía de Don Diego López de Haro, 45, 48011 Bilbao"
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

False positives (87):

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
- `en_demo_pdf` p2 LicensePlate [llm]: "7. TAX INFORMATION AND COMPLIANCE"
- `en_demo_pdf` p3 PersonName [heuristic]: "San Francisco"
- `en_demo_pdf` p3 PersonName [heuristic]: "American Arbitration Association"
- `en_demo_pdf` p3 PersonName [heuristic]: "Managing Partner"
- `en_demo_pdf` p3 PersonName [heuristic]: "Chief Executive Officer"
- `en_demo_pdf` p3 PersonName [heuristic]: "Senior Director"
- `en_demo_pdf` p3 CaseNumber [llm]: "10. GOVERNING LAW AND DISPUTE RESOLUTION"
- `en_demo_pdf` p3 Date [llm]: "11. ENTIRE AGREEMENT"
- `en_eu_share_purchase` p1 PersonName [heuristic]: "Purchase Price"
- `en_eu_share_purchase` p1 PersonName [heuristic]: "Managing Director"
- `en_eu_share_purchase` p1 CompanyName [llm]: "Mercedes-Benz E 300"
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
- `en_traps_llc_agreement` p1 PersonName [llm]: "Gantler v. Stephens"
- `en_traps_llc_agreement` p1 Address [llm]: "Entire Agreement"
- `es_carta_requerimiento` p1 PersonName [heuristic]: "Ciudad de México"
- `es_carta_requerimiento` p1 PersonName [heuristic]: "Estimada Sra"
- `es_contrato_arrendamiento` p1 PersonName [heuristic]: "En Madrid"
- `es_contrato_arrendamiento` p1 PersonName [heuristic]: "Arrendamientos Urbanos"
- `es_contrato_arrendamiento` p1 Address [llm]: "Agencia de Vivienda Social de la Comunidad de Madrid"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Juicio Verbal"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Ley de Enjuiciamiento Civil"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Código Civil"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "En Valencia"
- `es_demanda_reclamacion` p1 CaseNumber [llm]: "5 DE VALENCIA"
- `es_demanda_reclamacion` p1 CaseNumber [llm]: "2024/0031"
- `holdout_en_intake_memo` p1 PersonName [heuristic]: "Litigation Team"
- `holdout_en_intake_memo` p1 CreditCard [llm]: "1117"
- `holdout_en_intake_memo` p1 ClientId [llm]: "O.C.G.A. § 34-7-2"
- `holdout_es_acta_junta` p1 PersonName [heuristic]: "En Bilbao"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Prezado Sr"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Ofício de Registro de Imóveis de Belo Horizonte"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Lei do Inquilinato"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Belo Horizonte"
- `holdout_pt_notificacao` p1 MonetaryAmount [llm]: "10%"
- `holdout_pt_notificacao` p1 Date [llm]: "15 (quinze) dias"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "Banco do Brasil"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "Lei Geral de Proteção de Dados"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "Foro da Comarca de São Paulo"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "São Paulo"
- `pt_peticao_inicial` p1 PersonName [heuristic]: "Código de Processo Civil"
- `pt_peticao_inicial` p1 PersonName [heuristic]: "São Paulo"
- `pt_peticao_inicial` p1 CaseNumber [llm]: "STJ, REsp 1.737.412/SE"
- `pt_peticao_inicial` p1 SocialSecurityNumber [llm]: "Súmula 37 do STJ"
- `pt_procuracao_portugal` p1 PersonName [heuristic]: "Caro Sr"
- `pt_procuracao_portugal` p1 Address [llm]: "Rua de Santa Catarina"
- `pt_procuracao_portugal` p1 CompanyName [llm]: "Cartório Notarial de Lisboa"
- `pt_procuracao_portugal` p1 Date [llm]: "2024"

### Layer 3 alone false positives, run 1

- `en_court_complaint` p1 Address: "Contra Costa County"
- `en_demo_pdf` p2 LicensePlate: "7. TAX INFORMATION AND COMPLIANCE"
- `en_demo_pdf` p3 CaseNumber: "10. GOVERNING LAW AND DISPUTE RESOLUTION"
- `en_demo_pdf` p3 Date: "11. ENTIRE AGREEMENT"
- `en_eu_share_purchase` p1 CompanyName: "Mercedes-Benz E 300"
- `en_traps_llc_agreement` p1 PersonName: "Gantler v. Stephens"
- `en_traps_llc_agreement` p1 Address: "Entire Agreement"
- `en_traps_llc_agreement` p1 Address: "Entire Agreement"
- `es_contrato_arrendamiento` p1 Address: "Agencia de Vivienda Social de la Comunidad de Madrid"
- `es_demanda_reclamacion` p1 CaseNumber: "5 DE VALENCIA"
- `es_demanda_reclamacion` p1 CaseNumber: "2024/0031"
- `holdout_en_intake_memo` p1 CreditCard: "1117"
- `holdout_en_intake_memo` p1 ClientId: "O.C.G.A. § 34-7-2"
- `holdout_pt_notificacao` p1 MonetaryAmount: "10%"
- `holdout_pt_notificacao` p1 Date: "15 (quinze) dias"
- `pt_peticao_inicial` p1 CaseNumber: "STJ, REsp 1.737.412/SE"
- `pt_peticao_inicial` p1 SocialSecurityNumber: "Súmula 37 do STJ"
- `pt_procuracao_portugal` p1 Address: "Rua de Santa Catarina"
- `pt_procuracao_portugal` p1 CompanyName: "Cartório Notarial de Lisboa"
- `pt_procuracao_portugal` p1 Date: "2024"
