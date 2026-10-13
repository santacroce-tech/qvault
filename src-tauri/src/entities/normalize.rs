//! Text normalisation for entity resolution: accent folding, honorifics,
//! name connectors, company legal forms and address abbreviations in
//! English, Spanish and Portuguese (plus the German forms Qvault detects).
//!
//! Everything here is pure and deterministic; nothing is logged.

/// Lowercase and strip diacritics from Latin letters ("Ferrándiz" →
/// "ferrandiz", "João" → "joao", "Müller" → "muller"). Characters outside
/// the table are lowercased unchanged.
pub fn fold(s: &str) -> String {
    // "Sr.ª" / "Dr.ª" → "srª" / "drª" → "sra" / "dra".
    let s = s.replace(".ª", "ª").replace(". ª", "ª");
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        for l in c.to_lowercase() {
            match l {
                'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => out.push('a'),
                'ç' | 'ć' | 'č' => out.push('c'),
                'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => out.push('e'),
                'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => out.push('i'),
                'ñ' | 'ń' | 'ň' => out.push('n'),
                'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => out.push('o'),
                'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => out.push('u'),
                'ý' | 'ÿ' => out.push('y'),
                'ß' => out.push_str("ss"),
                'ł' => out.push('l'),
                'ś' | 'š' => out.push('s'),
                'ź' | 'ż' | 'ž' => out.push('z'),
                'ř' => out.push('r'),
                'ť' => out.push('t'),
                'ď' => out.push('d'),
                'æ' => out.push_str("ae"),
                'œ' => out.push_str("oe"),
                // ordinal indicators in "Dª", "Sr.ª", "nº"
                'ª' => out.push('a'),
                'º' => out.push('o'),
                // typographic apostrophes and dashes
                '’' | '‘' | '`' | '´' => out.push('\''),
                '‐' | '‑' | '‒' | '–' | '—' => out.push('-'),
                _ => out.push(l),
            }
        }
    }
    out
}

/// Honorifics and titles that precede a name, folded and without the final
/// dot, with the gender they imply (`Some('m')`, `Some('f')`, or `None` when
/// the title says nothing about it or is ambiguous across languages).
pub const HONORIFICS: &[(&str, Option<char>)] = &[
    // EN
    ("mr", Some('m')), ("mister", Some('m')), ("sir", Some('m')), ("lord", Some('m')),
    ("mrs", Some('f')), ("ms", Some('f')), ("miss", Some('f')), ("madam", Some('f')), ("dame", Some('f')), ("lady", Some('f')),
    ("mx", None), ("dr", None), ("doctor", None), ("prof", None), ("professor", None), ("hon", None),
    ("rev", None), ("judge", None), ("justice", None), ("esq", None),
    // ES
    ("sr", Some('m')), ("senor", Some('m')), ("don", Some('m')), ("dn", Some('m')),
    ("sra", Some('f')), ("senora", Some('f')), ("srta", Some('f')), ("senorita", Some('f')),
    ("dona", Some('f')), ("dna", Some('f')), ("da", None),
    ("dra", Some('f')), ("doctora", Some('f')), ("lic", None), ("licenciado", Some('m')), ("licenciada", Some('f')),
    ("ldo", Some('m')), ("lda", Some('f')), ("ing", None), ("arq", None), ("abog", None),
    // PT
    ("senhor", Some('m')), ("senhora", Some('f')), ("dom", Some('m')),
    ("profa", Some('f')), ("professora", Some('f')), ("eng", None), ("enga", Some('f')), ("engo", Some('m')),
    ("adv", None), ("exmo", Some('m')), ("exma", Some('f')), ("ilmo", Some('m')), ("ilma", Some('f')),
    ("d", None),
    // DE / FR (rare in the fixtures, cheap to support). "M." and "Me" are
    // left out on purpose: they collide with initials and names.
    ("herr", Some('m')), ("frau", Some('f')), ("mme", Some('f')), ("mlle", Some('f')),
];

/// Look up an honorific token (folded, trailing dots removed).
pub fn honorific(tok: &str) -> Option<Option<char>> {
    let t = tok.trim_end_matches('.');
    HONORIFICS.iter().find(|(h, _)| *h == t).map(|(_, g)| *g)
}

/// Particles inside names that carry no identity on their own.
pub const CONNECTORS: &[&str] = &[
    "de", "da", "do", "das", "dos", "del", "della", "di", "du", "la", "las", "los", "le", "van", "von", "der", "den",
    "ten", "ter", "y", "e", "i", "zu", "af", "st", "bin", "ibn",
];

pub fn is_connector(tok: &str) -> bool {
    CONNECTORS.contains(&tok)
}

/// Generational suffixes: "Jr." and "Sr." name different people; PT
/// "Filho"/"Neto"/"Júnior" are part of the name and distinguish a son from
/// his father.
pub const GENERATIONAL: &[&str] = &["jr", "junior", "ii", "iii", "iv", "filho", "neto", "sobrinho", "segundo", "hijo"];

pub fn generational(tok: &str) -> Option<&'static str> {
    let t = tok.trim_end_matches('.');
    let canon = match t {
        "jr" | "junior" => "jr",
        "ii" | "segundo" => "ii",
        "iii" => "iii",
        "iv" => "iv",
        "filho" | "hijo" => "filho",
        "neto" => "neto",
        "sobrinho" => "sobrinho",
        _ => return None,
    };
    Some(canon)
}

/// Post-nominal letters that are dropped ("Esq.", "PhD", "M.D.").
pub const POSTNOMINALS: &[&str] = &["esq", "phd", "md", "llm", "jd", "cpa", "mba", "oab", "dds", "rn"];

/// Company legal forms (folded, dots and slashes removed, spaces joined),
/// mapped to a canonical class so "Inc." and "Incorporated", or "S.A." and
/// "S/A", compare equal.
pub const LEGAL_FORMS: &[(&str, &str)] = &[
    ("llc", "llc"), ("lc", "llc"), ("pllc", "pllc"), ("llp", "llp"), ("lp", "lp"), ("lllp", "lllp"),
    ("inc", "inc"), ("incorporated", "inc"), ("corp", "inc"), ("corporation", "inc"),
    ("co", "co"), ("company", "co"), ("cia", "co"), ("companhia", "co"), ("compania", "co"),
    ("ltd", "ltd"), ("limited", "ltd"), ("plc", "plc"),
    ("gmbh", "gmbh"), ("ag", "ag"), ("kg", "kg"), ("ohg", "ohg"), ("ug", "ug"), ("gbr", "gbr"), ("ev", "ev"), ("kgaa", "kgaa"),
    ("sa", "sa"), ("sau", "sa"), ("saa", "sa"), ("sl", "sl"), ("slu", "sl"), ("srl", "srl"), ("sas", "sas"),
    ("sarl", "sarl"), ("spa", "spa"), ("sc", "sc"), ("scp", "scp"), ("ss", "ss"), ("sapi", "sapi"),
    ("cv", "cv"), ("rl", "rl"), ("sde", "sde"), ("sderl", "sderl"),
    ("ltda", "ltda"), ("lda", "ltda"), ("limitada", "ltda"), ("eireli", "eireli"), ("me", "me"), ("epp", "epp"), ("mei", "mei"),
    ("unipessoal", "unipessoal"), ("bv", "bv"), ("nv", "nv"), ("ab", "ab"), ("oy", "oy"), ("as", "as"), ("aps", "aps"),
    ("pty", "pty"), ("pte", "pte"), ("se", "se"),
];

/// Multi-word legal forms written out ("Sociedad Anónima"), folded.
pub const LEGAL_PHRASES: &[(&str, &str)] = &[
    ("sociedad anonima", "sa"), ("sociedade anonima", "sa"), ("sociedad limitada", "sl"),
    ("sociedad de responsabilidad limitada", "sl"), ("sociedade limitada", "ltda"),
    ("limited liability company", "llc"), ("public limited company", "plc"), ("and company", "co"),
    ("y compania", "co"), ("e companhia", "co"), ("sociedade unipessoal", "unipessoal"),
];

pub fn legal_form(tok: &str) -> Option<&'static str> {
    LEGAL_FORMS.iter().find(|(f, _)| *f == tok).map(|(_, c)| *c)
}

/// Words that never identify a company on their own (dropped from the core
/// when comparing): articles and "and".
pub const COMPANY_FILLERS: &[&str] = &["the", "el", "la", "los", "las", "o", "a", "os", "as", "and", "&", "y", "e", "und"];

/// Address abbreviations → canonical token.
pub const ADDRESS_ABBREV: &[(&str, &str)] = &[
    ("street", "st"), ("str", "st"), ("strasse", "st"), ("avenue", "av"), ("ave", "av"), ("avenida", "av"), ("avda", "av"),
    ("road", "rd"), ("boulevard", "blvd"), ("drive", "dr"), ("lane", "ln"), ("court", "ct"), ("place", "pl"),
    ("suite", "ste"), ("apartment", "apt"), ("floor", "fl"), ("rua", "r"), ("calle", "c"), ("travessa", "tv"),
    ("praca", "pc"), ("plaza", "pl"), ("paseo", "po"), ("numero", "n"), ("no", "n"), ("nro", "n"), ("num", "n"),
    ("piso", "fl"), ("andar", "fl"), ("sala", "ste"), ("conjunto", "cj"), ("bloco", "bl"), ("bloque", "bl"),
    ("north", "n"), ("south", "s"), ("east", "e"), ("west", "w"),
];

/// Split folded text into word tokens. Dots and commas end a token; '&',
/// apostrophes and internal hyphens are kept.
pub fn words(folded: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in folded.chars() {
        if c.is_alphanumeric() || c == '\'' || c == '-' || c == '&' {
            cur.push(c);
        } else {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.into_iter()
        .map(|w| w.trim_matches(|c| c == '-' || c == '\'').to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_accents_and_case() {
        assert_eq!(fold("María Inmaculada Ferrándiz"), "maria inmaculada ferrandiz");
        assert_eq!(fold("JOÃO CONCEIÇÃO"), "joao conceicao");
        assert_eq!(fold("Dª Müller–Straße"), "da muller-strasse");
    }

    #[test]
    fn honorifics_and_forms() {
        assert_eq!(honorific("mr."), Some(Some('m')));
        assert_eq!(honorific("sra"), Some(Some('f')));
        assert_eq!(honorific("dr"), Some(None));
        assert_eq!(honorific("maria"), None);
        assert_eq!(legal_form("incorporated"), Some("inc"));
        assert_eq!(legal_form("ltda"), Some("ltda"));
        assert_eq!(generational("jr."), Some("jr"));
        assert_eq!(words("santos, maria (adv.)"), vec!["santos", "maria", "adv"]);
    }
}
