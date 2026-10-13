# Eval: entity resolution (no model)

`eval/entities/variants.json`: 59 entities, 199 mentions (198 distinct strings). Pairwise = mention pairs of the same category. *Applied automatically* is what the app does on its own; *if every proposal is accepted* is the upper bound after review.

| system | applied automatically: pairwise P / R / F1 | B³ P / R | after review (correct proposals accepted): P / R | if every proposal is accepted: P / R / F1 | proposals (correct, precision) |
|---|---|---|---|---|---|
| rules + string similarity (no model) | 99.2 / 37.9 / 54.8 | 99.5 / 60.0 | 96.3 / 99.0 | 53.3 / 99.0 / 69.3 | 137 (93 correct, 67.9; 55 flagged ambiguous, 30 correct) |

By language (applied automatically, pairwise P / R):

| lang | P | R | pairs |
|---|---:|---:|---:|
| en | 98.0 | 30.0 | 160 |
| es | 100.0 | 36.8 | 76 |
| pt | 100.0 | 55.1 | 78 |

By category (pairwise P / R; applied → accepted):

| category | applied P / R | accepted P / R |
|---|---|---|
| Address | 100.0 / 55.6 | 100.0 / 100.0 |
| CompanyName | 100.0 / 81.0 | 92.1 / 100.0 |
| Email | 100.0 / 100.0 | 100.0 / 100.0 |
| IBAN | 100.0 / 100.0 | 100.0 / 100.0 |
| PersonName | 98.3 / 24.4 | 46.7 / 98.7 |
| PhoneNumber | 100.0 / 100.0 | 100.0 / 100.0 |
| SocialSecurityNumber | 100.0 / 100.0 | 100.0 / 100.0 |
| TaxId | 100.0 / 100.0 | 100.0 / 100.0 |

Proposals:

- ✓ `Maria Santos` ↔ `Maria Fernanda Santos` (name-extension)
- ✗ `João Santos` ↔ `João Pedro Silva Santos` (name-extension) — en_joao_santos vs pt_joao_pedro_silva_santos
- ✗ `João Santos` ↔ `João Pedro Silva Santos Filho` (name-extension) — en_joao_santos vs pt_joao_pedro_silva_santos_filho
- ✓ `Dana Holloway` ↔ `Dana Rae Holloway` (name-extension)
- ✓ `Jonathan Whitmore` ↔ `Jonathan Paul Whitmore` (name-extension)
- ✗ `Jonathan Whitmore` ↔ `Jonathan Whitmore Jr.` (name-extension) — en_jonathan_whitmore vs en_jonathan_whitmore_jr
- ✗ `Jonathan Paul Whitmore` ↔ `Jonathan Whitmore Jr.` (name-extension) — en_jonathan_whitmore vs en_jonathan_whitmore_jr
- ✓ `María Inmaculada Ferrándiz Olmedo` ↔ `María Ferrándiz` (name-extension)
- ✓ `Lucía Méndez Soto` ↔ `Lucia Mendez` (name-extension)
- ✓ `Álvaro Ruiz de la Fuente` ↔ `Alvaro Ruiz` (name-extension)
- ✗ `Alvaro Ruiz` ↔ `Álvaro Ruiz Soto` (name-extension) — es_alvaro_ruiz vs es_alvaro_ruiz_soto
- ✗ `João Pedro Silva Santos` ↔ `João Pedro Silva Santos Filho` (name-extension) — pt_joao_pedro_silva_santos vs pt_joao_pedro_silva_santos_filho
- ✓ `Marcos Antônio de Oliveira` ↔ `Marcos Oliveira` (name-extension)
- ✓ `Beatriz Costa` ↔ `Beatriz Lima Costa` (name-extension)
- ✗ `Beatriz Costa` ↔ `Beatriz Costa Neves` (name-extension) — pt_beatriz_costa vs pt_beatriz_costa_neves
- ✗ `William Hart` ↔ `William Harte` (similar-spelling) — en_william_hart vs en_william_harte
- ✓ `Sarah Lindqvist` ↔ `Sarah Lindquist` (similar-spelling)
- ✓ `Jan Kowalski` ↔ `Jan Kowalsky` (similar-spelling)
- ✓ `Thiago Almeida` ↔ `Tiago Almeida` (similar-spelling)
- ✓ `Maria Santos` ↔ `Maria F. Santos` (initial, ambiguous)
- ✓ `Dana Holloway` ↔ `Dana R. Holloway` (initial, ambiguous)
- ✓ `Jonathan P. Whitmore` ↔ `Jonathan Whitmore` (initial)
- ✗ `Jonathan P. Whitmore` ↔ `Jonathan Whitmore Jr.` (initial) — en_jonathan_whitmore vs en_jonathan_whitmore_jr
- ✗ `Jonathan Whitmore` ↔ `Jonathan P. Whitmore, Jr.` (initial) — en_jonathan_whitmore vs en_jonathan_whitmore_jr
- ✓ `Jonathan Whitmore Jr.` ↔ `Jonathan P. Whitmore, Jr.` (initial)
- ✓ `Robert Feeney` ↔ `Robert J. Feeney` (initial)
- ✓ `Marcos Oliveira` ↔ `Sr. Marcos A. de Oliveira` (initial)
- ✓ `Maria Santos` ↔ `M. Santos` (initial)
- ✓ `M. Santos` ↔ `Maria F. Santos` (initial)
- ✓ `M. Santos` ↔ `Maria Fernanda Santos` (initial)
- ✓ `Maria F. Santos` ↔ `Maria Fernanda Santos` (initial, ambiguous)
- ✓ `João Santos` ↔ `J. Santos` (initial, ambiguous)
- ✗ `João Santos` ↔ `J. P. Silva Santos` (initial, ambiguous) — en_joao_santos vs pt_joao_pedro_silva_santos
- ✓ `J. Santos` ↔ `James Santos` (initial, ambiguous)
- ✗ `J. Santos` ↔ `João Pedro Silva Santos` (initial, ambiguous) — en_joao_santos/en_james_santos vs pt_joao_pedro_silva_santos
- ✗ `J. Santos` ↔ `J. P. Silva Santos` (initial, ambiguous) — en_joao_santos/en_james_santos vs pt_joao_pedro_silva_santos
- ✗ `J. Santos` ↔ `João Pedro Silva Santos Filho` (initial, ambiguous) — en_joao_santos/en_james_santos vs pt_joao_pedro_silva_santos_filho
- ✗ `James Santos` ↔ `J. P. Silva Santos` (initial, ambiguous) — en_james_santos vs pt_joao_pedro_silva_santos
- ✓ `Dana Holloway` ↔ `D. Holloway` (initial, ambiguous)
- ✓ `D. Holloway` ↔ `Dana R. Holloway` (initial, ambiguous)
- ✓ `D. Holloway` ↔ `Dana Rae Holloway` (initial, ambiguous)
- ✓ `Dana R. Holloway` ↔ `Dana Rae Holloway` (initial, ambiguous)
- ✓ `Peter Holloway` ↔ `P. Holloway` (initial, ambiguous)
- ✓ `Jonathan P. Whitmore` ↔ `J. P. Whitmore` (initial)
- ✓ `Jonathan P. Whitmore` ↔ `Jonathan Paul Whitmore` (initial)
- ✗ `Jonathan P. Whitmore` ↔ `Jonathan P. Whitmore, Jr.` (initial) — en_jonathan_whitmore vs en_jonathan_whitmore_jr
- ✓ `Jonathan Whitmore` ↔ `J. P. Whitmore` (initial)
- ✓ `J. P. Whitmore` ↔ `Jonathan Paul Whitmore` (initial)
- ✗ `J. P. Whitmore` ↔ `Jonathan Whitmore Jr.` (initial) — en_jonathan_whitmore vs en_jonathan_whitmore_jr
- ✗ `J. P. Whitmore` ↔ `Jonathan P. Whitmore, Jr.` (initial) — en_jonathan_whitmore vs en_jonathan_whitmore_jr
- ✗ `Jonathan Paul Whitmore` ↔ `Jonathan P. Whitmore, Jr.` (initial) — en_jonathan_whitmore vs en_jonathan_whitmore_jr
- ✓ `Sarah Lindqvist` ↔ `S. Lindqvist` (initial)
- ✓ `Dr. Amara Okafor` ↔ `A. Okafor` (initial)
- ✓ `Chidi Okafor` ↔ `C. Okafor` (initial)
- ✓ `Jan Kowalski` ↔ `J. Kowalski` (initial)
- ✓ `William Hart` ↔ `W. Hart` (initial)
- ✓ `Carmen Ferrándiz Ruiz` ↔ `C. Ferrándiz Ruiz` (initial)
- ✓ `José Luis García Pérez` ↔ `J. L. García Pérez` (initial)
- ✓ `Lucía Méndez Soto` ↔ `L. Méndez Soto` (initial)
- ✓ `Lucia Mendez` ↔ `L. Méndez Soto` (initial)
- ✓ `Álvaro Ruiz de la Fuente` ↔ `Á. Ruiz de la Fuente` (initial)
- ✓ `Á. Ruiz de la Fuente` ↔ `Alvaro Ruiz` (initial)
- ✓ `Ana Paula da Conceição Ribeiro` ↔ `A. P. Ribeiro` (initial, ambiguous)
- ✗ `A. P. Ribeiro` ↔ `Paulo Ribeiro` (initial, ambiguous) — pt_ana_paula_ribeiro vs pt_paulo_ribeiro
- ✓ `João Pedro Silva Santos` ↔ `J. P. Silva Santos` (initial, ambiguous)
- ✗ `J. P. Silva Santos` ↔ `João Pedro Silva Santos Filho` (initial, ambiguous) — pt_joao_pedro_silva_santos vs pt_joao_pedro_silva_santos_filho
- ✓ `Marcos Antônio de Oliveira` ↔ `Sr. Marcos A. de Oliveira` (initial)
- ✓ `Beatriz Costa` ↔ `B. Lima Costa` (initial)
- ✓ `Beatriz Lima Costa` ↔ `B. Lima Costa` (initial)
- ✓ `Thiago Almeida` ↔ `T. Almeida` (initial)
- ✓ `Tiago Almeida` ↔ `T. Almeida` (initial)
- ✓ `Rua das Flores, 123, São Paulo` ↔ `Rua das Flores, nº 123, São Paulo - SP` (similar-address)
- ✓ `2287 Juniper Hollow Road, Oakland, CA 94611` ↔ `2287 Juniper Hollow Road` (similar-address)
- ✓ `Maria Santos` ↔ `Ms. Santos` (surname-only, ambiguous)
- ✗ `Maria Santos` ↔ `Mr. Santos` (surname-only, ambiguous) — en_maria_santos vs en_joao_santos
- ✓ `Ms. Santos` ↔ `M. Santos` (surname-only, ambiguous)
- ✓ `Ms. Santos` ↔ `Maria F. Santos` (surname-only, ambiguous)
- ✓ `Ms. Santos` ↔ `Maria Fernanda Santos` (surname-only, ambiguous)
- ✗ `Ms. Santos` ↔ `João Santos` (surname-only, ambiguous) — en_maria_santos vs en_joao_santos
- ✗ `Ms. Santos` ↔ `J. Santos` (surname-only, ambiguous) — en_maria_santos vs en_joao_santos/en_james_santos
- ✗ `Ms. Santos` ↔ `James Santos` (surname-only, ambiguous) — en_maria_santos vs en_james_santos
- ✗ `Ms. Santos` ↔ `João Pedro Silva Santos` (surname-only, ambiguous) — en_maria_santos vs pt_joao_pedro_silva_santos
- ✗ `Ms. Santos` ↔ `J. P. Silva Santos` (surname-only, ambiguous) — en_maria_santos vs pt_joao_pedro_silva_santos
- ✗ `Ms. Santos` ↔ `João Pedro Silva Santos Filho` (surname-only, ambiguous) — en_maria_santos vs pt_joao_pedro_silva_santos_filho
- ✗ `M. Santos` ↔ `Mr. Santos` (surname-only, ambiguous) — en_maria_santos vs en_joao_santos
- ✗ `Maria F. Santos` ↔ `Mr. Santos` (surname-only) — en_maria_santos vs en_joao_santos
- ✗ `Maria Fernanda Santos` ↔ `Mr. Santos` (surname-only) — en_maria_santos vs en_joao_santos
- ✓ `João Santos` ↔ `Mr. Santos` (surname-only, ambiguous)
- ✓ `Mr. Santos` ↔ `J. Santos` (surname-only, ambiguous)
- ✗ `Mr. Santos` ↔ `James Santos` (surname-only, ambiguous) — en_joao_santos vs en_james_santos
- ✗ `Mr. Santos` ↔ `João Pedro Silva Santos` (surname-only, ambiguous) — en_joao_santos vs pt_joao_pedro_silva_santos
- ✗ `Mr. Santos` ↔ `J. P. Silva Santos` (surname-only, ambiguous) — en_joao_santos vs pt_joao_pedro_silva_santos
- ✗ `Mr. Santos` ↔ `João Pedro Silva Santos Filho` (surname-only, ambiguous) — en_joao_santos vs pt_joao_pedro_silva_santos_filho
- ✓ `Dana Holloway` ↔ `Ms. Holloway` (surname-only, ambiguous)
- ✗ `Dana Holloway` ↔ `Mr. Holloway` (surname-only, ambiguous) — en_dana_holloway vs en_peter_holloway
- ✓ `Ms. Holloway` ↔ `D. Holloway` (surname-only, ambiguous)
- ✓ `Ms. Holloway` ↔ `Dana R. Holloway` (surname-only, ambiguous)
- ✓ `Ms. Holloway` ↔ `Dana Rae Holloway` (surname-only, ambiguous)
- ✗ `Ms. Holloway` ↔ `Peter Holloway` (surname-only, ambiguous) — en_dana_holloway vs en_peter_holloway
- ✗ `Ms. Holloway` ↔ `P. Holloway` (surname-only, ambiguous) — en_dana_holloway vs en_peter_holloway
- ✗ `D. Holloway` ↔ `Mr. Holloway` (surname-only, ambiguous) — en_dana_holloway vs en_peter_holloway
- ✗ `Dana R. Holloway` ↔ `Mr. Holloway` (surname-only) — en_dana_holloway vs en_peter_holloway
- ✗ `Dana Rae Holloway` ↔ `Mr. Holloway` (surname-only) — en_dana_holloway vs en_peter_holloway
- ✓ `Peter Holloway` ↔ `Mr. Holloway` (surname-only, ambiguous)
- ✓ `Mr. Holloway` ↔ `P. Holloway` (surname-only, ambiguous)
- ✓ `Sarah Lindqvist` ↔ `Ms. Lindqvist` (surname-only)
- ✓ `Ms. Lindqvist` ↔ `S. Lindqvist` (surname-only)
- ✓ `Sarah Lin` ↔ `Ms. Lin` (surname-only)
- ✓ `Robert Feeney` ↔ `Mr. Feeney` (surname-only, ambiguous)
- ✓ `Bob Feeney` ↔ `Mr. Feeney` (surname-only, ambiguous)
- ✓ `Mr. Feeney` ↔ `Robert J. Feeney` (surname-only, ambiguous)
- ✓ `Dr. Amara Okafor` ↔ `Dr. Okafor` (surname-only, ambiguous)
- ✓ `A. Okafor` ↔ `Dr. Okafor` (surname-only, ambiguous)
- ✗ `Dr. Okafor` ↔ `Chidi Okafor` (surname-only, ambiguous) — en_amara_okafor vs en_chidi_okafor
- ✗ `Dr. Okafor` ↔ `C. Okafor` (surname-only, ambiguous) — en_amara_okafor vs en_chidi_okafor
- ✓ `Jan Kowalski` ↔ `Mr. Kowalski` (surname-only)
- ✓ `J. Kowalski` ↔ `Mr. Kowalski` (surname-only)
- ✓ `Lucía Méndez Soto` ↔ `Sra. Méndez` (surname-only)
- ✓ `Lucia Mendez` ↔ `Sra. Méndez` (surname-only)
- ✓ `Sra. Méndez` ↔ `L. Méndez Soto` (surname-only)
- ✓ `Ana Paula da Conceição Ribeiro` ↔ `Sra. Ribeiro` (surname-only)
- ✓ `Sra. Ribeiro` ↔ `A. P. Ribeiro` (surname-only, ambiguous)
- ✓ `María Inmaculada Ferrándiz Olmedo` ↔ `Sra. Ferrándiz Olmedo` (partial-name)
- ✓ `María Inmaculada Ferrándiz Olmedo` ↔ `Inmaculada Ferrándiz` (partial-name)
- ✓ `Carmen Ferrándiz Ruiz` ↔ `Sra. Ferrándiz Ruiz` (partial-name)
- ✓ `Sra. Ferrándiz Ruiz` ↔ `C. Ferrándiz Ruiz` (partial-name)
- ✓ `José Luis García Pérez` ↔ `Sr. García Pérez` (partial-name)
- ✓ `J. L. García Pérez` ↔ `Sr. García Pérez` (partial-name)
- ✓ `José García López` ↔ `Sr. García López` (partial-name)
- ✓ `Álvaro Ruiz de la Fuente` ↔ `Sr. Ruiz de la Fuente` (partial-name)
- ✓ `Á. Ruiz de la Fuente` ↔ `Sr. Ruiz de la Fuente` (partial-name)
- ✓ `Álvaro Ruiz Soto` ↔ `Sr. Ruiz Soto` (partial-name)
- ✓ `Beatriz Costa Neves` ↔ `Sra. Costa Neves` (partial-name)
- ✓ `Meridian Consulting LLC` ↔ `Meridian` (partial-name)
- ✗ `Meridian` ↔ `Meridian Consulting GmbH` (partial-name) — en_meridian_consulting_llc vs de_meridian_consulting_gmbh
- ✓ `Atlas Digital Solutions Inc.` ↔ `Atlas Digital` (partial-name)
- ✓ `Atlas Digital Solutions Inc.` ↔ `ADS` (acronym)

Wrong automatic merges:

- none

Variants left apart (not merged, not proposed), per entity:

- `en_william_hart`: [Bill Hart] · [William Hart, W. Hart, Hart, William]
