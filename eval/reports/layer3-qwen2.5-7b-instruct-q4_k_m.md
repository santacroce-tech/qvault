# Eval: Layer 3 with `qwen2.5-7b-instruct-q4_k_m`

assessFit: **likely-fits** · load 2.3 s · 18 pages · 1 run(s) · prompt `p3` · chunks of 150 words, 20 overlap · post-processing: default

## Overall (overlap matching; mean ± sd over runs)

| | P | R | F1 | covered R | EN F1 | ES F1 | PT F1 |
|---|---|---|---|---|---|---|---|
| Layer 3 alone | 98.7 | 81.8 | 89.4 | 81.8 | 87.0 | 91.6 | 93.3 |
| Layers 1+2 | 84.9 | 83.1 | 84.0 | 75.5 | 80.0 | 88.2 | 90.2 |
| Layers 1+2+3 | 85.9 | 96.7 | 91.0 | 89.2 | 89.2 | 92.8 | 94.0 |

Speed: 18.95 s/page (Layer 3 only; 53 chunks/run, 0 failed chunks over all runs: 0 engine errors, 0 unparseable answers). Peak RSS of the sidecar: 4995 MB.

Run-to-run: span-set Jaccard – (pairs of runs), identical raw answers – of chunks.

### Exact / overlap / covered, run 1

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| Layer 3 alone | 461 | 381 | 98.7 / 81.8 / 89.4 | 90.0 / 74.4 / 81.5 | 81.8 | 93.6 |
| Layers 1+2+3 | 461 | 517 | 85.9 / 96.7 / 91.0 | 76.8 / 86.1 / 81.2 | 89.2 | 97.1 |
| Layers 1+2+3 – dev | 401 | 459 | 85.4 / 98.0 / 91.3 | 77.6 / 88.8 / 82.8 | 91.8 | 97.2 |
| Layers 1+2+3 – holdout | 60 | 58 | 89.7 / 88.3 / 89.0 | 70.7 / 68.3 / 69.5 | 71.7 | 96.2 |

### Prompt versions (run 1 each)

| | labels | preds | overlap P / R / F1 | exact P / R / F1 | covered R | type acc. |
|---|---:|---:|---|---|---:|---:|
| `p3` Layer 3 alone (run 1) | 461 | 381 | 98.7 / 81.8 / 89.4 | 90.0 / 74.4 / 81.5 | 81.8 | 93.6 |
| `p3` Layers 1+2+3 (run 1) | 461 | 517 | 85.9 / 96.7 / 91.0 | 76.8 / 86.1 / 81.2 | 89.2 | 97.1 |

### Post-processing ablation (replayed on run 1's answers)

| config | L3 P | L3 R | L3 F1 | L1+2+3 P | L1+2+3 R | L1+2+3 F1 |
|---|---|---|---|---|---|---|
| default | 98.7 | 81.8 | 89.4 | 85.9 | 96.7 | 91.0 |
| Phase 1B post-processing | 97.8 | 77.9 | 86.7 | 85.1 | 94.4 | 89.5 |
| − stop-list | 97.2 | 81.8 | 88.8 | 84.9 | 96.7 | 90.4 |
| − Last, First variants | 98.7 | 81.1 | 89.0 | 85.8 | 96.1 | 90.7 |
| − honorific + surname | 98.7 | 80.7 | 88.8 | 85.7 | 95.7 | 90.4 |
| − page-wide mapping | 98.7 | 80.0 | 88.4 | 85.9 | 96.5 | 90.9 |
| − accent folding | 98.7 | 81.8 | 89.4 | 85.9 | 96.7 | 91.0 |
| − hyphen line-wrap | 98.7 | 81.6 | 89.3 | 85.9 | 96.5 | 90.9 |

### What happened to the model's items (run 1)

```
{
  "items": 398,
  "salvaged_chunks": 0,
  "invalid_items": 0,
  "unknown_category": 0,
  "empty_text": 0,
  "implausible": 10,
  "stoplisted": 2,
  "not_in_chunk": 5,
  "accepted": 379,
  "variant_hits": 17,
  "page_wide_hits": 35,
  "covered_by_layers_1_2": 0
}
```

### Layer 3 alone by category (all runs pooled; for confidence calibration)

| category | preds | P (overlap) | R (overlap) | type acc. |
|---|---:|---:|---:|---:|
| Address | 28 | 96.4 | 93.1 | 100.0 |
| BankAccount | 16 | 100.0 | 66.7 | 100.0 |
| CaseNumber | 9 | 77.8 | 83.3 | 80.0 |
| ClientId | 0 | – | 60.0 | 0.0 |
| CompanyName | 27 | 100.0 | 81.1 | 90.0 |
| CourtFileNumber | 1 | 100.0 | 100.0 | 100.0 |
| CreditCard | 7 | 100.0 | 100.0 | 87.5 |
| Date | 41 | 100.0 | 55.6 | 100.0 |
| DateOfBirth | 7 | 100.0 | 88.9 | 87.5 |
| DriversLicense | 2 | 100.0 | 100.0 | 50.0 |
| Email | 22 | 100.0 | 88.0 | 100.0 |
| IBAN | 3 | 100.0 | 88.9 | 37.5 |
| IpAddress | 6 | 100.0 | 100.0 | 100.0 |
| LicensePlate | 6 | 100.0 | 100.0 | 100.0 |
| MedicalRecordNumber | 4 | 100.0 | 100.0 | 100.0 |
| MonetaryAmount | 38 | 100.0 | 82.6 | 100.0 |
| NationalId | 26 | 100.0 | 100.0 | 100.0 |
| PassportNumber | 2 | 100.0 | 100.0 | 66.7 |
| PersonName | 101 | 98.0 | 85.0 | 100.0 |
| PhoneNumber | 25 | 100.0 | 100.0 | 100.0 |
| SocialSecurityNumber | 2 | 100.0 | 100.0 | 66.7 |
| TaxId | 6 | 100.0 | 75.0 | 50.0 |
| Url | 2 | 100.0 | 66.7 | 100.0 |

Agreement (run 1): detections confirmed by two overlapping chunks: 65/65 correct (100.0); single-chunk: 311/316 (98.4).

### Layer 3 additions to Layers 1+2 by category (all runs pooled; confidence calibration)

| category | added | correct | P |
|---|---:|---:|---:|
| Address | 4 | 3 | 75.0 |
| BankAccount | 4 | 4 | 100.0 |
| CaseNumber | 3 | 1 | 33.3 |
| CompanyName | 5 | 5 | 100.0 |
| Date | 3 | 3 | 100.0 |
| DateOfBirth | 1 | 1 | 100.0 |
| Email | 1 | 1 | 100.0 |
| NationalId | 2 | 2 | 100.0 |
| PersonName | 45 | 43 | 95.6 |
| **all** | 68 | 63 | 92.6 |

### Layers 1+2+3 by category, run 1

| category | labels | L1+2 P / R | L3 P / R | L1+2+3 P / R |
|---|---:|---|---|---|
| Address | 29 | 100.0 / 89.7 | 96.4 / 93.1 | 96.7 / 100.0 |
| BankAccount | 15 | 100.0 / 73.3 | 100.0 / 66.7 | 100.0 / 100.0 |
| CaseNumber | 6 | 100.0 / 83.3 | 77.8 / 83.3 | 75.0 / 100.0 |
| ClientId | 5 | 100.0 / 80.0 | – / 60.0 | 100.0 / 80.0 |
| CompanyName | 37 | 92.0 / 81.1 | 100.0 / 81.1 | 93.3 / 100.0 |
| CourtFileNumber | 1 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| CreditCard | 8 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| Date | 72 | 100.0 / 91.7 | 100.0 / 55.6 | 100.0 / 95.8 |
| DateOfBirth | 9 | 100.0 / 88.9 | 100.0 / 88.9 | 100.0 / 100.0 |
| DriversLicense | 4 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| Email | 25 | 100.0 / 96.0 | 100.0 / 88.0 | 100.0 / 100.0 |
| IBAN | 9 | 100.0 / 100.0 | 100.0 / 88.9 | 100.0 / 100.0 |
| IpAddress | 6 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| LicensePlate | 5 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| MedicalRecordNumber | 4 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| MonetaryAmount | 46 | 100.0 / 100.0 | 100.0 / 82.6 | 100.0 / 100.0 |
| NationalId | 17 | 100.0 / 88.2 | 100.0 / 100.0 | 100.0 / 100.0 |
| PassportNumber | 3 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| PersonName | 113 | 50.0 / 54.0 | 98.0 / 85.0 | 61.6 / 90.3 |
| PhoneNumber | 25 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| SocialSecurityNumber | 3 | 100.0 / 100.0 | 100.0 / 100.0 | 100.0 / 100.0 |
| TaxId | 16 | 100.0 / 100.0 | 100.0 / 75.0 | 100.0 / 100.0 |
| Url | 3 | 100.0 / 100.0 | 100.0 / 66.7 | 100.0 / 100.0 |

### Layers 1+2+3 errors, run 1

False negatives (50):

- `en_court_complaint` p1 PersonName (partial): "Marcus T. Feeney"
- `en_court_complaint` p1 CompanyName (partial): "Marsh & Okonjo LLP"
- `en_demand_letter` p1 CompanyName (partial): "Okafor Whitfield PLLC"
- `en_demand_letter` p1 PersonName (partial): "Harriet Pemberton-Shaw"
- `en_demand_letter` p1 ClientId (partial): "NGM-24-0073318"
- `en_demand_letter` p1 MedicalRecordNumber (partial): "MRN 00482913"
- `en_demand_letter` p1 PersonName (missed): "Lindqvist"
- `en_demand_letter` p1 PersonName (missed): "T. R. Lindqvist"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 PersonName (missed): "Felix Brandauer"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine France SAS"
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_eu_share_purchase` p1 CompanyName (partial): "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 PersonName (partial): "Henrike Vollmer-Baumgart"
- `en_traps_llc_agreement` p1 PersonName (missed): "Constance Varga-Ellison"
- `en_traps_llc_agreement` p1 PersonName (missed): "Rupert Ashdown"
- `en_traps_llc_agreement` p1 PersonName (missed): "Imelda Szabo"
- `en_traps_llc_agreement` p1 PersonName (partial): "Constance Varga-Ellison"
- `es_carta_requerimiento` p1 CompanyName (partial): "Bufete Arriaga & Solís, S.C."
- `es_carta_requerimiento` p1 PersonName (missed): "Ramírez de la Garza"
- `es_contrato_arrendamiento` p1 PersonName (partial): "Tomás Ezequiel Barroso\nQuintanilla"
- `es_contrato_arrendamiento` p1 CompanyName (partial): "Inversiones Quintanilla Hermanos, S.L."
- `es_contrato_arrendamiento` p1 PersonName (partial): "Tomás E. Barroso Quintanilla"
- `es_demanda_reclamacion` p1 CompanyName (partial): "Reformas Turia Integral, S.L.U."
- `es_demanda_reclamacion` p1 PersonName (missed): "Aleixandre"
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

False positives (73):

- `en_court_complaint` p1 PersonName [heuristic]: "See Burlington"
- `en_court_complaint` p1 PersonName [heuristic]: "Civil Rights Act"
- `en_demand_letter` p1 PersonName [heuristic]: "Claims Department"
- `en_demand_letter` p1 PersonName [heuristic]: "Claim No"
- `en_demand_letter` p1 PersonName [heuristic]: "Dear Claims Representative"
- `en_demand_letter` p1 PersonName [heuristic]: "New York"
- `en_demand_letter` p1 PersonName [heuristic]: "Because Mr"
- `en_demand_letter` p1 Address [llm]: "Atlantic Avenue and Court Street"
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
- `es_carta_requerimiento` p1 PersonName [heuristic]: "Ciudad de México"
- `es_carta_requerimiento` p1 PersonName [heuristic]: "Estimada Sra"
- `es_contrato_arrendamiento` p1 PersonName [heuristic]: "En Madrid"
- `es_contrato_arrendamiento` p1 PersonName [heuristic]: "Arrendamientos Urbanos"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Juicio Verbal"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Ley de Enjuiciamiento Civil"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "Código Civil"
- `es_demanda_reclamacion` p1 PersonName [heuristic]: "En Valencia"
- `es_demanda_reclamacion` p1 CaseNumber [llm]: "STS 1/2015, de 27 de enero"
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
- `pt_peticao_inicial` p1 PersonName [llm]: "EXCELENTÍSSIMO SENHOR DOUTOR JUIZ DE DIREITO"
- `pt_peticao_inicial` p1 CaseNumber [llm]: "REsp 1.737.412/SE"
- `pt_procuracao_portugal` p1 PersonName [heuristic]: "Caro Sr"

### Layer 3 alone false positives, run 1

- `en_demand_letter` p1 Address: "Atlantic Avenue and Court Street"
- `en_settlement_agreement` p1 PersonName: "Achterberg"
- `es_demanda_reclamacion` p1 CaseNumber: "STS 1/2015, de 27 de enero"
- `pt_peticao_inicial` p1 PersonName: "EXCELENTÍSSIMO SENHOR DOUTOR JUIZ DE DIREITO"
- `pt_peticao_inicial` p1 CaseNumber: "REsp 1.737.412/SE"
