# Eval: Layer 3 with `qwen3-8b-q4_k_m`

assessFit: **likely-fits** · load 2.6 s · 18 pages · 1 run(s) · prompt `p3` · chunks of 150 words, 20 overlap · post-processing: default

## Overall (overlap matching; mean ± sd over runs)

| | P | R | F1 | covered R | EN F1 | ES F1 | PT F1 |
|---|---|---|---|---|---|---|---|
| Layer 3 alone | 97.5 | 86.1 | 91.5 | 85.7 | 93.4 | 90.6 | 86.9 |
| Layers 1+2 | 84.9 | 83.1 | 84.0 | 75.5 | 80.0 | 88.2 | 90.2 |
| Layers 1+2+3 | 85.1 | 97.2 | 90.7 | 89.4 | 89.7 | 91.3 | 93.0 |

Speed: 21.76 s/page (Layer 3 only; 53 chunks/run, 0 failed chunks over all runs: 0 engine errors, 0 unparseable answers). Peak RSS of the sidecar: 5521 MB.

Run-to-run: span-set Jaccard – (pairs of runs), identical raw answers – of chunks.

### Exact / overlap / covered, run 1

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| Layer 3 alone | 461 | 405 | 97.5 / 86.1 / 91.5 | 92.1 / 80.9 / 86.1 | 85.7 | 97.5 |
| Layers 1+2+3 | 461 | 524 | 85.1 / 97.2 / 90.7 | 76.5 / 87.0 / 81.4 | 89.4 | 97.3 |
| Layers 1+2+3 – dev | 401 | 462 | 84.8 / 98.0 / 91.0 | 77.3 / 89.0 / 82.7 | 91.5 | 97.7 |
| Layers 1+2+3 – holdout | 60 | 62 | 87.1 / 91.7 / 89.3 | 71.0 / 73.3 / 72.1 | 75.0 | 94.5 |

### Prompt versions (run 1 each)

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| `p3` Layer 3 alone (run 1) | 461 | 405 | 97.5 / 86.1 / 91.5 | 92.1 / 80.9 / 86.1 | 85.7 | 97.5 |
| `p3` Layers 1+2+3 (run 1) | 461 | 524 | 85.1 / 97.2 / 90.7 | 76.5 / 87.0 / 81.4 | 89.4 | 97.3 |

### Post-processing ablation (replayed on run 1's answers)

| config | L3 P | L3 R | L3 F1 | L1+2+3 P | L1+2+3 R | L1+2+3 F1 |
|---|---|---|---|---|---|---|
| default | 97.5 | 86.1 | 91.5 | 85.1 | 97.2 | 90.7 |
| Phase 1B post-processing | 96.6 | 80.0 | 87.5 | 84.1 | 93.7 | 88.7 |
| − stop-list | 96.1 | 86.1 | 90.8 | 84.2 | 97.2 | 90.2 |
| − Last, First variants | 97.5 | 84.8 | 90.7 | 85.0 | 96.3 | 90.3 |
| − honorific + surname | 97.5 | 83.9 | 90.2 | 84.8 | 95.0 | 89.6 |
| − page-wide mapping | 97.4 | 83.1 | 89.7 | 85.0 | 96.5 | 90.4 |
| − accent folding | 97.5 | 86.1 | 91.5 | 85.1 | 97.2 | 90.7 |
| − hyphen line-wrap | 97.5 | 85.9 | 91.3 | 85.1 | 97.0 | 90.6 |

### What happened to the model's items (run 1)

```
{
  "items": 446,
  "salvaged_chunks": 1,
  "invalid_items": 0,
  "unknown_category": 0,
  "empty_text": 0,
  "implausible": 5,
  "stoplisted": 2,
  "not_in_chunk": 8,
  "accepted": 414,
  "variant_hits": 18,
  "page_wide_hits": 37,
  "covered_by_layers_1_2": 0
}
```

### Layer 3 alone by category (all runs pooled; for confidence calibration)

| category | preds | P (overlap) | R (overlap) | type acc. |
|---|---:|---:|---:|---:|
| Address | 26 | 100.0 | 89.7 | 100.0 |
| BankAccount | 11 | 100.0 | 66.7 | 100.0 |
| CaseNumber | 9 | 88.9 | 83.3 | 100.0 |
| ClientId | 5 | 60.0 | 100.0 | 60.0 |
| CompanyName | 30 | 100.0 | 81.1 | 100.0 |
| CourtFileNumber | 0 | – | 100.0 | 0.0 |
| CreditCard | 10 | 90.0 | 100.0 | 100.0 |
| Date | 61 | 96.7 | 83.3 | 100.0 |
| DateOfBirth | 9 | 100.0 | 100.0 | 100.0 |
| DriversLicense | 3 | 100.0 | 100.0 | 75.0 |
| Email | 23 | 100.0 | 92.0 | 100.0 |
| IBAN | 6 | 100.0 | 100.0 | 66.7 |
| IpAddress | 6 | 100.0 | 100.0 | 100.0 |
| LicensePlate | 6 | 100.0 | 100.0 | 100.0 |
| MedicalRecordNumber | 4 | 100.0 | 100.0 | 100.0 |
| MonetaryAmount | 43 | 90.7 | 84.8 | 100.0 |
| NationalId | 18 | 100.0 | 88.2 | 100.0 |
| PassportNumber | 3 | 100.0 | 100.0 | 100.0 |
| PersonName | 90 | 100.0 | 79.6 | 100.0 |
| PhoneNumber | 23 | 100.0 | 92.0 | 100.0 |
| SocialSecurityNumber | 3 | 100.0 | 100.0 | 100.0 |
| TaxId | 13 | 100.0 | 100.0 | 81.2 |
| Url | 3 | 100.0 | 100.0 | 100.0 |

Agreement (run 1): detections confirmed by two overlapping chunks: 77/77 correct (100.0); single-chunk: 318/328 (97.0).

### Layer 3 additions to Layers 1+2 by category (all runs pooled; confidence calibration)

| category | added | correct | P |
|---|---:|---:|---:|
| Address | 2 | 2 | 100.0 |
| BankAccount | 4 | 4 | 100.0 |
| CaseNumber | 3 | 2 | 66.7 |
| ClientId | 2 | 0 | 0.0 |
| CompanyName | 7 | 7 | 100.0 |
| CreditCard | 1 | 0 | 0.0 |
| Date | 7 | 5 | 71.4 |
| DateOfBirth | 1 | 1 | 100.0 |
| Email | 1 | 1 | 100.0 |
| MonetaryAmount | 4 | 0 | 0.0 |
| NationalId | 2 | 2 | 100.0 |
| PersonName | 41 | 41 | 100.0 |
| **all** | 75 | 65 | 86.7 |

### Layers 1+2+3 by category, run 1

| category | labels | L1+2 P / R | L3 P / R | L1+2+3 P / R |
|---|---:|---|---|---|
| Address | 29 | 100.0 / 89.7 | 100.0 / 89.7 | 100.0 / 96.6 |
| BankAccount | 15 | 100.0 / 73.3 | 100.0 / 66.7 | 100.0 / 100.0 |
| CaseNumber | 6 | 100.0 / 83.3 | 88.9 / 83.3 | 87.5 / 100.0 |
| ClientId | 5 | 100.0 / 80.0 | 60.0 / 100.0 | 60.0 / 100.0 |
| CompanyName | 37 | 92.0 / 81.1 | 100.0 / 81.1 | 93.8 / 100.0 |
| CourtFileNumber | 1 | 100.0 / 100.0 | – / 100.0 | 100.0 / 100.0 |
| CreditCard | 8 | 100.0 / 100.0 | 90.0 / 100.0 | 88.9 / 100.0 |
| Date | 72 | 100.0 / 91.7 | 96.7 / 83.3 | 97.3 / 98.6 |
| DateOfBirth | 9 | 100.0 / 88.9 | 100.0 / 100.0 | 100.0 / 100.0 |
| DriversLicense | 4 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| Email | 25 | 100.0 / 96.0 | 100.0 / 92.0 | 100.0 / 100.0 |
| IBAN | 9 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| IpAddress | 6 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| LicensePlate | 5 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| MedicalRecordNumber | 4 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| MonetaryAmount | 46 | 100.0 / 100.0 | 90.7 / 84.8 | 92.0 / 100.0 |
| NationalId | 17 | 100.0 / 88.2 | 100.0 / 88.2 | 100.0 / 100.0 |
| PassportNumber | 3 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| PersonName | 113 | 50.0 / 54.0 | 100.0 / 79.6 | 61.8 / 90.3 |
| PhoneNumber | 25 | 100.0 / 100.0 | 100.0 / 92.0 | 100.0 / 100.0 |
| SocialSecurityNumber | 3 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| TaxId | 16 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| Url | 3 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |

### Layers 1+2+3 errors, run 1

False negatives (49):

- `en_court_complaint` p1 PersonName (partial): "Marcus T. Feeney"
- `en_court_complaint` p1 CompanyName (partial): "Marsh & Okonjo LLP"
- `en_demand_letter` p1 CompanyName (partial): "Okafor Whitfield PLLC"
- `en_demand_letter` p1 PersonName (partial): "Harriet Pemberton-Shaw"
- `en_demand_letter` p1 ClientId (partial): "NGM-24-0073318"
- `en_demand_letter` p1 MedicalRecordNumber (partial): "MRN 00482913"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 PersonName (missed): "Felix Brandauer"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine France SAS"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_settlement_agreement` p1 PersonName (missed): "S. Achterberg"
- `en_settlement_agreement` p1 PersonName (missed): "Achterberg"
- `en_settlement_agreement` p1 PersonName (missed): "Rafferty\nOkonkwo-Bell"
- `en_settlement_agreement` p1 PersonName (partial): "Ms Achterberg"
- `en_traps_llc_agreement` p1 PersonName (partial): "Constance Varga-Ellison"
- `es_carta_requerimiento` p1 CompanyName (partial): "Bufete Arriaga & Solís, S.C."
- `es_carta_requerimiento` p1 PersonName (missed): "Ramírez de la Garza"
- `es_contrato_arrendamiento` p1 PersonName (partial): "Tomás Ezequiel Barroso\nQuintanilla"
- `es_contrato_arrendamiento` p1 CompanyName (partial): "Inversiones Quintanilla Hermanos, S.L."
- `es_contrato_arrendamiento` p1 PersonName (partial): "Tomás E. Barroso Quintanilla"
- `es_demanda_reclamacion` p1 PersonName (missed): "Beatriz Coloma Ruano"
- `es_demanda_reclamacion` p1 CompanyName (partial): "Reformas Turia Integral, S.L.U."
- `es_demanda_reclamacion` p1 PersonName (missed): "Álvaro Sendra Peiró"
- `holdout_en_intake_memo` p1 PersonName (partial): "Naomi Fairbanks-Oyelaran"
- `holdout_en_intake_memo` p1 Date (missed): "2025.03.18"
- `holdout_en_intake_memo` p1 PersonName (partial): "Desmond Achebe-Kowalczyk"
- `holdout_en_intake_memo` p1 PersonName (partial): "Desmond Achebe-Kowalczyk"
- `holdout_en_intake_memo` p1 Address (partial): "83 Lantern Hill Terrace, Unit 12, Marietta, GA 30064-3317"
- `holdout_en_intake_memo` p1 Address (missed): "2210 Rue Sainte-Catherine Ouest, Montréal, QC H3H 1M7"
- `holdout_en_intake_memo` p1 PersonName (missed): "Desmond"
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
- `pt_procuracao_portugal` p1 PersonName (missed): "J. M. T. Fonseca"

False positives (78):

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
- `en_traps_llc_agreement` p1 MonetaryAmount [llm]: "10,000"
- `en_traps_llc_agreement` p1 MonetaryAmount [llm]: "2,500"
- `es_carta_requerimiento` p1 PersonName [heuristic]: "Ciudad de México"
- `es_carta_requerimiento` p1 PersonName [heuristic]: "Estimada Sra"
- `es_carta_requerimiento` p1 MonetaryAmount [llm]: "quince (15) días hábiles"
- `es_contrato_arrendamiento` p1 PersonName [heuristic]: "En Madrid"
- `es_contrato_arrendamiento` p1 PersonName [heuristic]: "Arrendamientos Urbanos"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Juicio Verbal"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Ley de Enjuiciamiento Civil"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Código Civil"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "En Valencia"
- `es_demanda_reclamacion` p1 ClientId [llm]: "2024/0031"
- `es_demanda_reclamacion` p1 ClientId [llm]: "6."
- `holdout_en_intake_memo` p1 PersonName [heuristic]: "Litigation Team"
- `holdout_en_intake_memo` p1 CreditCard [llm]: "ending 1117"
- `holdout_es_acta_junta` p1 PersonName [heuristic]: "En Bilbao"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Prezado Sr"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Ofício de Registro de Imóveis de Belo Horizonte"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Lei do Inquilinato"
- `holdout_pt_notificacao` p1 PersonName [heuristic]: "Belo Horizonte"
- `holdout_pt_notificacao` p1 Date [llm]: "15 (quinze) dias"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "Banco do Brasil"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "Lei Geral de Proteção de Dados"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "Foro da Comarca de São Paulo"
- `pt_contrato_prestacao` p1 PersonName [heuristic]: "São Paulo"
- `pt_contrato_prestacao` p1 MonetaryAmount [llm]: "10%"
- `pt_peticao_inicial` p1 PersonName [heuristic]: "Código de Processo Civil"
- `pt_peticao_inicial` p1 PersonName [heuristic]: "São Paulo"
- `pt_peticao_inicial` p1 CaseNumber [llm]: "REsp 1.737.412/SE"
- `pt_procuracao_portugal` p1 PersonName [heuristic]: "Caro Sr"

### Layer 3 alone false positives, run 1

- `en_garnishment_affidavit` p1 Date: "09/27"
- `en_traps_llc_agreement` p1 MonetaryAmount: "10,000"
- `en_traps_llc_agreement` p1 MonetaryAmount: "2,500"
- `es_carta_requerimiento` p1 MonetaryAmount: "quince (15) días hábiles"
- `es_demanda_reclamacion` p1 ClientId: "2024/0031"
- `es_demanda_reclamacion` p1 ClientId: "6."
- `holdout_en_intake_memo` p1 CreditCard: "ending 1117"
- `holdout_pt_notificacao` p1 Date: "15 (quinze) dias"
- `pt_contrato_prestacao` p1 MonetaryAmount: "10%"
- `pt_peticao_inicial` p1 CaseNumber: "REsp 1.737.412/SE"
