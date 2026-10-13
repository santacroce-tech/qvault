# Eval: known-entity suggestions on top of Layers 1–3 (`qwen3-1.7b-q8_0`)

Failed chunks: 0.

## On top of Layers 1–2

| knowledge base | labels missed before | recovered | suggestions | false suggestions | overlap R before → after | covered R before → after | overlap P before → after |
|---|---:|---:|---:|---:|---|---|---|
| same document, unreviewed | 78 | 13 | 23 | 10 | 83.1 → 85.9 | 75.5 → 77.7 | 84.9 → 83.5 |
| same document, after review | 78 | 13 | 13 | 0 | 83.1 → 85.9 | 75.5 → 77.7 | 84.9 → 85.3 |
| approved in an earlier document (upper bound) | 78 | 65 | 67 | 1 | 83.1 → 97.2 | 75.5 → 88.7 | 84.9 → 86.6 |

### same document, unreviewed

By rule (suggestions / false): honorific-surname 3/0, initial-surname 1/0, reordered 1/0, same-text 18/10

Recovered (13):

- `en_court_complaint` p1 PersonName: "Holloway"
- `en_demand_letter` p1 PersonName: "J. Whitfield"
- `en_demo_pdf` p1 PersonName: "Roberto  \n  Fernandes"
- `en_eu_share_purchase` p1 CompanyName: "Zeeland Marine Technologies B.V."
- `en_garnishment_affidavit` p1 PersonName: "LORNA FETTERMAN"
- `en_settlement_agreement` p1 CompanyName: "HALCYON DATA SYSTEMS LTD"
- `en_traps_llc_agreement` p1 PersonName: "Constance Varga-Ellison"
- `es_demanda_reclamacion` p1 PersonName: "LLORENTE ESCRIVÁ, Nuria"
- `es_demanda_reclamacion` p1 PersonName: "Álvaro Sendra Peiró"
- `holdout_pt_notificacao` p1 PersonName: "Vasconcelos"
- `pt_contrato_prestacao` p1 CompanyName: "MOURA DANTAS CONSULTORIA TRIBUTÁRIA EIRELI"
- `pt_peticao_inicial` p1 PersonName: "Paixão"
- `pt_procuracao_portugal` p1 PersonName: "Fonseca, Joaquim"

False suggestions (10):

- `en_demand_letter` p1 PersonName (same-text): "New York"
- `en_demand_letter` p1 PersonName (same-text): "New York"
- `en_demo_pdf` p1 PersonName (same-text): "Managing Partner"
- `en_demo_pdf` p1 PersonName (same-text): "Chief Executive Officer"
- `en_demo_pdf` p1 PersonName (same-text): "Senior Director"
- `en_traps_llc_agreement` p1 CompanyName (same-text): "Delaware limited"
- `en_traps_llc_agreement` p1 PersonName (same-text): "Internal Revenue Code"
- `en_traps_llc_agreement` p1 PersonName (same-text): "Entire Agreement"
- `es_demanda_reclamacion` p1 PersonName (same-text): "JUICIO VERBAL"
- `pt_peticao_inicial` p1 PersonName (same-text): "SÃO PAULO"

### same document, after review

By rule (suggestions / false): honorific-surname 3/0, initial-surname 1/0, reordered 1/0, same-text 8/0

Recovered (13):

- `en_court_complaint` p1 PersonName: "Holloway"
- `en_demand_letter` p1 PersonName: "J. Whitfield"
- `en_demo_pdf` p1 PersonName: "Roberto  \n  Fernandes"
- `en_eu_share_purchase` p1 CompanyName: "Zeeland Marine Technologies B.V."
- `en_garnishment_affidavit` p1 PersonName: "LORNA FETTERMAN"
- `en_settlement_agreement` p1 CompanyName: "HALCYON DATA SYSTEMS LTD"
- `en_traps_llc_agreement` p1 PersonName: "Constance Varga-Ellison"
- `es_demanda_reclamacion` p1 PersonName: "LLORENTE ESCRIVÁ, Nuria"
- `es_demanda_reclamacion` p1 PersonName: "Álvaro Sendra Peiró"
- `holdout_pt_notificacao` p1 PersonName: "Vasconcelos"
- `pt_contrato_prestacao` p1 CompanyName: "MOURA DANTAS CONSULTORIA TRIBUTÁRIA EIRELI"
- `pt_peticao_inicial` p1 PersonName: "Paixão"
- `pt_procuracao_portugal` p1 PersonName: "Fonseca, Joaquim"

False suggestions (0):


### approved in an earlier document (upper bound)

By rule (suggestions / false): initial-surname 3/0, reordered 5/0, same-text 59/1

Recovered (65):

- `en_court_complaint` p1 PersonName: "HOLLOWAY, Dana"
- `en_court_complaint` p1 PersonName: "Marcus T. Feeney"
- `en_court_complaint` p1 PersonName: "Holloway"
- `en_court_complaint` p1 PersonName: "Feeney"
- `en_court_complaint` p1 PersonName: "Priya\nRamaswamy"
- `en_court_complaint` p1 PersonName: "Feeney"
- `en_court_complaint` p1 PersonName: "Ramaswamy"
- `en_demand_letter` p1 PersonName: "Tobias R. Lindqvist"
- `en_demand_letter` p1 PersonName: "Tobias R. Lindqvist"
- `en_demand_letter` p1 PersonName: "Lindqvist"
- `en_demand_letter` p1 PersonName: "Lindqvist"
- `en_demand_letter` p1 PersonName: "Lindqvist"
- `en_demand_letter` p1 PersonName: "J. Whitfield"
- `en_demand_letter` p1 PersonName: "T. R. Lindqvist"
- `en_demo_pdf` p1 Email: "j.richardson@atlas-  \n  digital.com"
- `en_demo_pdf` p1 PersonName: "Roberto  \n  Fernandes"
- `en_eu_share_purchase` p1 Address: "Westerdok 118, 1013 BH Amsterdam"
- `en_eu_share_purchase` p1 PersonName: "Felix Brandauer"
- `en_eu_share_purchase` p1 CaseNumber: "16 O 248/24"
- `en_eu_share_purchase` p1 PersonName: "Joost van der Linde"
- `en_eu_share_purchase` p1 CompanyName: "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 PersonName: "Joost van der Linde"
- `en_garnishment_affidavit` p1 CompanyName: "Desert Sun Credit Union"
- `en_garnishment_affidavit` p1 PersonName: "LORNA FETTERMAN"
- `en_garnishment_affidavit` p1 CompanyName: "Desert Sun Credit Union"
- `en_garnishment_affidavit` p1 BankAccount: "0047-221983"
- `en_garnishment_affidavit` p1 BankAccount: "884 120 33571"
- `en_garnishment_affidavit` p1 BankAccount: "3266 0519 8820"
- `en_settlement_agreement` p1 PersonName: "Siobhan Achterberg"
- `en_settlement_agreement` p1 PersonName: "S. Achterberg"
- `en_settlement_agreement` p1 PersonName: "Achterberg"
- `en_settlement_agreement` p1 PersonName: "Rafferty\nOkonkwo-Bell"
- `en_settlement_agreement` p1 PersonName: "Ms Achterberg"
- `en_settlement_agreement` p1 PersonName: "Gideon Farquharson"
- `en_settlement_agreement` p1 PersonName: "Achterberg, Siobhan"
- `en_settlement_agreement` p1 PersonName: "Farquharson, Gideon"
- `en_settlement_agreement` p1 PersonName: "SIOBHAN ACHTERBERG"
- `en_settlement_agreement` p1 PersonName: "GIDEON FARQUHARSON"
- `en_settlement_agreement` p1 CompanyName: "HALCYON DATA SYSTEMS LTD"
- `en_traps_llc_agreement` p1 PersonName: "Constance Varga-Ellison"
- `en_traps_llc_agreement` p1 PersonName: "Rupert Ashdown"
- `en_traps_llc_agreement` p1 PersonName: "Imelda Szabo"
- `es_carta_requerimiento` p1 PersonName: "Ramírez de la Garza"
- `es_contrato_arrendamiento` p1 PersonName: "Rogelio Quintanilla Mateu"
- `es_demanda_reclamacion` p1 PersonName: "LLORENTE ESCRIVÁ, Nuria"
- `es_demanda_reclamacion` p1 PersonName: "Beatriz Coloma Ruano"
- `es_demanda_reclamacion` p1 PersonName: "Fermín Aleixandre Bou"
- `es_demanda_reclamacion` p1 PersonName: "Aleixandre"
- `es_demanda_reclamacion` p1 PersonName: "Álvaro Sendra Peiró"
- `holdout_en_intake_memo` p1 CompanyName: "Pellucid Freight Systems, Inc."
- `holdout_en_intake_memo` p1 ClientId: "M-25-0331"
- `holdout_en_intake_memo` p1 Address: "2210 Rue Sainte-Catherine Ouest, Montréal, QC H3H 1M7"
- `holdout_en_intake_memo` p1 NationalId: "046 454 286"
- `holdout_en_intake_memo` p1 PersonName: "Desmond"
- `holdout_en_intake_memo` p1 BankAccount: "000123456789"
- `holdout_es_acta_junta` p1 CompanyName: "TALLERES METÁLICOS ARGAÑA, S.L."
- `holdout_es_acta_junta` p1 Address: "C/ Gran Vía de Don Diego López de Haro, 45, 48011 Bilbao"
- `holdout_es_acta_junta` p1 PersonName: "Zabaleta"
- `holdout_es_acta_junta` p1 PersonName: "Beloki"
- `holdout_es_acta_junta` p1 PersonName: "Koldo Iturbe Larrañaga"
- `holdout_pt_notificacao` p1 NationalId: "39053344705"
- `holdout_pt_notificacao` p1 PersonName: "Vasconcelos"
- `pt_contrato_prestacao` p1 CompanyName: "MOURA DANTAS CONSULTORIA TRIBUTÁRIA EIRELI"
- `pt_peticao_inicial` p1 PersonName: "Paixão"
- `pt_procuracao_portugal` p1 PersonName: "Fonseca, Joaquim"

False suggestions (1):

- `en_settlement_agreement` p1 PersonName (same-text): "Achterberg"

## On top of Layers 1–3 (qwen3-1.7b-q8_0)

| knowledge base | labels missed before | recovered | suggestions | false suggestions | overlap R before → after | covered R before → after | overlap P before → after |
|---|---:|---:|---:|---:|---|---|---|
| same document, unreviewed | 32 | 5 | 15 | 9 | 93.1 → 94.1 | 85.0 → 85.9 | 84.9 → 83.6 |
| same document, after review | 32 | 5 | 6 | 0 | 93.1 → 94.1 | 85.0 → 85.9 | 84.9 → 85.1 |
| approved in an earlier document (upper bound) | 32 | 26 | 27 | 0 | 93.1 → 98.7 | 85.0 → 90.7 | 84.9 → 85.7 |

### same document, unreviewed

By rule (suggestions / false): honorific-surname 1/0, initial-surname 1/0, same-text 13/9

Recovered (5):

- `en_demand_letter` p1 PersonName: "T. R. Lindqvist"
- `en_eu_share_purchase` p1 CompanyName: "Zeeland Marine Technologies B.V."
- `en_garnishment_affidavit` p1 PersonName: "LORNA FETTERMAN"
- `es_demanda_reclamacion` p1 PersonName: "Álvaro Sendra Peiró"
- `pt_peticao_inicial` p1 PersonName: "Paixão"

False suggestions (9):

- `en_demand_letter` p1 PersonName (same-text): "New York"
- `en_demand_letter` p1 PersonName (same-text): "New York"
- `en_demo_pdf` p1 PersonName (same-text): "Managing Partner"
- `en_demo_pdf` p1 PersonName (same-text): "Chief Executive Officer"
- `en_demo_pdf` p1 PersonName (same-text): "Senior Director"
- `en_traps_llc_agreement` p1 CompanyName (same-text): "Delaware limited"
- `en_traps_llc_agreement` p1 PersonName (same-text): "Internal Revenue Code"
- `en_traps_llc_agreement` p1 PersonName (same-text): "Entire Agreement"
- `es_demanda_reclamacion` p1 PersonName (same-text): "JUICIO VERBAL"

### same document, after review

By rule (suggestions / false): honorific-surname 1/0, initial-surname 1/0, same-text 4/0

Recovered (5):

- `en_demand_letter` p1 PersonName: "T. R. Lindqvist"
- `en_eu_share_purchase` p1 CompanyName: "Zeeland Marine Technologies B.V."
- `en_garnishment_affidavit` p1 PersonName: "LORNA FETTERMAN"
- `es_demanda_reclamacion` p1 PersonName: "Álvaro Sendra Peiró"
- `pt_peticao_inicial` p1 PersonName: "Paixão"

False suggestions (0):


### approved in an earlier document (upper bound)

By rule (suggestions / false): initial-surname 1/0, same-text 26/0

Recovered (26):

- `en_court_complaint` p1 PersonName: "Priya\nRamaswamy"
- `en_court_complaint` p1 PersonName: "Ramaswamy"
- `en_demand_letter` p1 PersonName: "T. R. Lindqvist"
- `en_eu_share_purchase` p1 Address: "Westerdok 118, 1013 BH Amsterdam"
- `en_eu_share_purchase` p1 PersonName: "Felix Brandauer"
- `en_eu_share_purchase` p1 CaseNumber: "16 O 248/24"
- `en_eu_share_purchase` p1 PersonName: "Joost van der Linde"
- `en_eu_share_purchase` p1 CompanyName: "Zeeland Marine Technologies B.V."
- `en_eu_share_purchase` p1 PersonName: "Joost van der Linde"
- `en_garnishment_affidavit` p1 CompanyName: "Desert Sun Credit Union"
- `en_garnishment_affidavit` p1 PersonName: "LORNA FETTERMAN"
- `en_garnishment_affidavit` p1 CompanyName: "Desert Sun Credit Union"
- `en_garnishment_affidavit` p1 BankAccount: "884 120 33571"
- `en_garnishment_affidavit` p1 BankAccount: "3266 0519 8820"
- `en_settlement_agreement` p1 PersonName: "Rafferty\nOkonkwo-Bell"
- `es_carta_requerimiento` p1 PersonName: "Ramírez de la Garza"
- `es_contrato_arrendamiento` p1 PersonName: "Rogelio Quintanilla Mateu"
- `es_demanda_reclamacion` p1 PersonName: "Aleixandre"
- `es_demanda_reclamacion` p1 PersonName: "Álvaro Sendra Peiró"
- `holdout_en_intake_memo` p1 Address: "2210 Rue Sainte-Catherine Ouest, Montréal, QC H3H 1M7"
- `holdout_en_intake_memo` p1 PersonName: "Desmond"
- `holdout_es_acta_junta` p1 PersonName: "Zabaleta"
- `holdout_es_acta_junta` p1 PersonName: "Beloki"
- `holdout_es_acta_junta` p1 PersonName: "Koldo Iturbe Larrañaga"
- `holdout_pt_notificacao` p1 NationalId: "39053344705"
- `pt_peticao_inicial` p1 PersonName: "Paixão"

False suggestions (0):

