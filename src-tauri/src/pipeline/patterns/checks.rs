//! Checksum and structure validators used by Layer 1 patterns. Each returns
//! `Some((accepted_len, confidence))` for a match it accepts (the accepted
//! length may be shorter than the match, e.g. an IBAN followed by an
//! unrelated group of characters) or `None` to drop the match.

pub type Check = fn(&str) -> Option<(usize, f64)>;

fn digits(s: &str) -> Vec<u32> {
    s.chars().filter_map(|c| c.to_digit(10)).collect()
}

pub fn luhn_ok(ds: &[u32]) -> bool {
    if ds.len() < 12 {
        return false;
    }
    let sum: u32 = ds
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| if i % 2 == 1 { let x = d * 2; if x > 9 { x - 9 } else { x } } else { d })
        .sum();
    sum % 10 == 0
}

/// Undelimited card numbers: Luhn required (a bare 13–19 digit run is too
/// often an account or reference number otherwise).
pub fn card_plain(m: &str) -> Option<(usize, f64)> {
    luhn_ok(&digits(m)).then(|| (m.len(), 0.95))
}

/// Card numbers written in groups (4-4-4-4, Amex 4-6-5): the grouping is
/// itself strong evidence, so a failed Luhn check (typos, made-up examples
/// such as the one in landing/demo.pdf) lowers the confidence instead of
/// dropping the match.
pub fn card_grouped(m: &str) -> Option<(usize, f64)> {
    let ds = digits(m);
    let iin_ok = matches!(ds.first(), Some(2..=6));
    if !iin_ok {
        return None;
    }
    Some((m.len(), if luhn_ok(&ds) { 0.95 } else { 0.80 }))
}

/// ABA routing number: 9 digits, weighted checksum 3-7-1, and a first pair
/// in the ranges the Federal Reserve assigns (00–12, 21–32, 61–72, 80).
pub fn aba_routing(m: &str) -> Option<(usize, f64)> {
    let d = digits(m);
    if d.len() != 9 {
        return None;
    }
    let sum = 3 * (d[0] + d[3] + d[6]) + 7 * (d[1] + d[4] + d[7]) + (d[2] + d[5] + d[8]);
    let prefix = d[0] * 10 + d[1];
    let prefix_ok = prefix <= 12 || (21..=32).contains(&prefix) || (61..=72).contains(&prefix) || prefix == 80;
    (sum % 10 == 0 && prefix_ok && d.iter().any(|&x| x != 0)).then(|| (m.len(), 0.85))
}

/// Same checksum, used when a "routing/ABA" label precedes the number.
pub fn aba_routing_labelled(m: &str) -> Option<(usize, f64)> {
    let d = digits(m);
    if d.len() != 9 {
        return None;
    }
    let sum = 3 * (d[0] + d[3] + d[6]) + 7 * (d[1] + d[4] + d[7]) + (d[2] + d[5] + d[8]);
    (sum % 10 == 0).then(|| (m.len(), 0.95))
}

const ISO_COUNTRIES: &str = "AD AE AF AG AI AL AM AO AR AT AU AW AZ BA BB BD BE BF BG BH BI BJ BM BN BO BR BS BT BW BY BZ \
CA CD CF CG CH CI CL CM CN CO CR CU CV CY CZ DE DJ DK DM DO DZ EC EE EG ES ET FI FJ FO FR GA GB GD GE GG GH GI GL GM GN \
GQ GR GT GW GY HK HN HR HT HU ID IE IL IM IN IQ IR IS IT JE JM JO JP KE KG KH KM KN KR KW KY KZ LA LB LC LI LK LR LS LT \
LU LV LY MA MC MD ME MG MK ML MM MN MO MR MT MU MV MW MX MY MZ NA NE NG NI NL NO NP NZ OM PA PE PG PH PK PL PS PT PY QA \
RO RS RU RW SA SB SC SD SE SG SI SK SL SM SN SO SR SS ST SV SY SZ TD TG TH TJ TL TM TN TO TR TT TW TZ UA UG US UY UZ VA \
VC VE VG VN VU WS XK YE ZA ZM ZW";

pub fn is_country(cc: &str) -> bool {
    cc.len() == 2 && ISO_COUNTRIES.split(' ').any(|c| c == cc)
}

/// SWIFT/BIC: 4 letters bank, ISO country, 2 location characters, optional
/// 3-character branch. Without a "SWIFT/BIC" label (checked by the labelled
/// pattern) an all-letter 8-character word is far more likely to be an
/// ALL-CAPS word, so the bare pattern needs a digit in the location code or
/// an explicit branch.
pub fn bic_bare(m: &str) -> Option<(usize, f64)> {
    if !(m.len() == 8 || m.len() == 11) || !is_country(&m[4..6]) {
        return None;
    }
    let loc_digit = m[6..8].chars().any(|c| c.is_ascii_digit());
    let branch = m.len() == 11 && (m.ends_with("XXX") || m[8..].chars().any(|c| c.is_ascii_digit()));
    (loc_digit || branch).then(|| (m.len(), 0.85))
}

pub fn bic_labelled(m: &str) -> Option<(usize, f64)> {
    ((m.len() == 8 || m.len() == 11) && is_country(&m[4..6])).then(|| (m.len(), 0.95))
}

fn iban_mod97(compact: &str) -> bool {
    if compact.len() < 15 || compact.len() > 34 {
        return false;
    }
    let rearranged = format!("{}{}", &compact[4..], &compact[..4]);
    let mut rem: u64 = 0;
    for c in rearranged.chars() {
        let v = match c {
            '0'..='9' => c as u64 - '0' as u64,
            'A'..='Z' => c as u64 - 'A' as u64 + 10,
            _ => return false,
        };
        rem = if v >= 10 { (rem * 100 + v) % 97 } else { (rem * 10 + v) % 97 };
    }
    rem == 1
}

/// IBAN: ISO country, then mod-97 == 1. Spaced IBANs can run into a
/// following group of characters, so the longest prefix that ends on a
/// group boundary and validates is accepted.
pub fn iban(m: &str) -> Option<(usize, f64)> {
    if !is_country(&m[..2]) {
        return None;
    }
    let mut ends: Vec<usize> = m.char_indices().filter(|(_, c)| *c == ' ').map(|(i, _)| i).collect();
    ends.push(m.len());
    for &end in ends.iter().rev() {
        let compact: String = m[..end].chars().filter(|c| !c.is_whitespace()).collect();
        if iban_mod97(&compact) {
            return Some((end, 0.95));
        }
    }
    None
}

const DNI_LETTERS: &[u8] = b"TRWAGMYFPDXBNJZSQVHLCKE";

/// Spanish DNI (8 digits + letter) and NIE (X/Y/Z + 7 digits + letter),
/// validated by the mod-23 control letter.
pub fn es_dni_nie(m: &str) -> Option<(usize, f64)> {
    let b = m.as_bytes();
    if b.len() != 9 {
        return None;
    }
    let lead = match b[0] {
        b'X' => "0",
        b'Y' => "1",
        b'Z' => "2",
        b'0'..=b'9' => "",
        _ => return None,
    };
    let num_part = if lead.is_empty() { &m[..8] } else { &m[1..8] };
    let n: u64 = format!("{}{}", lead, num_part).parse().ok()?;
    (DNI_LETTERS[(n % 23) as usize] == b[8]).then(|| (m.len(), 0.95))
}

/// Brazilian CPF (11 digits, two mod-11 check digits). Used to type
/// Privacy Filter's untyped `account_number` spans; Layer 1's own CPF pattern
/// relies on the 000.000.000-00 layout instead.
pub fn br_cpf(m: &str) -> Option<(usize, f64)> {
    let d = digits(m);
    if d.len() != 11 || d.iter().all(|&x| x == d[0]) {
        return None;
    }
    let dv = |n: usize| -> u32 {
        let s: u32 = (0..n).map(|i| d[i] * (n as u32 + 1 - i as u32)).sum();
        (s * 10 % 11) % 10
    };
    (dv(9) == d[9] && dv(10) == d[10]).then(|| (m.len(), 0.95))
}

/// Brazilian CNPJ (14 digits, two mod-11 check digits).
pub fn br_cnpj(m: &str) -> Option<(usize, f64)> {
    let d = digits(m);
    if d.len() != 14 || d.iter().all(|&x| x == d[0]) {
        return None;
    }
    let dv = |w: &[u32]| -> u32 {
        let s: u32 = w.iter().zip(&d).map(|(a, b)| a * b).sum();
        if s % 11 < 2 { 0 } else { 11 - s % 11 }
    };
    let ok = dv(&[5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2]) == d[12] && dv(&[6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2]) == d[13];
    ok.then(|| (m.len(), 0.95))
}

/// Portuguese NIF / NIPC (9 digits, mod-11 check digit, valid first digit).
pub fn pt_nif(m: &str) -> Option<(usize, f64)> {
    let d = digits(m);
    if d.len() != 9 || ![1, 2, 3, 5, 6, 8, 9].contains(&d[0]) {
        return None;
    }
    let s: u32 = (0..8).map(|i| d[i] * (9 - i as u32)).sum();
    let c = 11 - s % 11;
    ((if c >= 10 { 0 } else { c }) == d[8]).then(|| (m.len(), 0.95))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn br_and_pt_numbers() {
        assert!(br_cpf("222.333.444-05").is_some());
        assert!(br_cpf("22233344405").is_some());
        assert!(br_cpf("222.333.444-06").is_none());
        assert!(br_cpf("111.111.111-11").is_none(), "repeated digits are not issued");
        assert!(br_cnpj("11.222.333/0001-81").is_some());
        assert!(br_cnpj("11.222.333/0001-82").is_none());
        assert!(pt_nif("123456789").is_some());
        assert!(pt_nif("123 456 789").is_some());
        assert!(pt_nif("123456780").is_none());
        assert!(pt_nif("423456789").is_none(), "no NIF starts with 4");
    }

    #[test]
    fn luhn_and_cards() {
        assert!(card_plain("4111111111111111").is_some());
        assert!(card_plain("4012888888881881").is_some());
        assert!(card_plain("4111111111111112").is_none());
        assert_eq!(card_grouped("4111 1111 1111 1111"), Some((19, 0.95)));
        assert_eq!(card_grouped("3782 822463 10005"), Some((17, 0.95)));
        // landing/demo.pdf's card fails Luhn: kept, at lower confidence.
        assert_eq!(card_grouped("4532-8901-2345-6789"), Some((19, 0.80)));
        assert!(card_grouped("1234-5678-9012-3456").is_none(), "no card network starts with 1");
    }

    #[test]
    fn routing_numbers() {
        assert!(aba_routing("021000021").is_some());
        assert!(aba_routing("121000358").is_some());
        assert!(aba_routing("026009593").is_some());
        assert!(aba_routing("021000022").is_none());
        assert!(aba_routing("548271936").is_none());
        assert!(aba_routing("000000000").is_none());
    }

    #[test]
    fn bics() {
        assert!(bic_bare("FNBCUS6S").is_some());
        assert!(bic_bare("COBADEFFXXX").is_some());
        assert!(bic_bare("PROVIDED").is_none(), "an ALL-CAPS word");
        assert!(bic_bare("DEUTDEFF").is_none(), "all letters: needs a SWIFT/BIC label");
        assert!(bic_labelled("DEUTDEFF").is_some());
        assert!(bic_labelled("ABCDQQ12").is_none(), "QQ is not a country");
    }

    #[test]
    fn ibans() {
        for ok in [
            "DE89 3704 0044 0532 0130 00",
            "GB82 WEST 1234 5698 7654 32",
            "ES91 2100 0418 4502 0005 1332",
            "PT50 0002 0123 1234 5678 9015 4",
            "FR14 2004 1010 0505 0001 3M02 606",
            "NL91ABNA0417164300",
            "BE68 5390 0754 7034",
            "DE44 5001 0517 5407 3249 31",
        ] {
            assert_eq!(iban(ok), Some((ok.len(), 0.95)), "{}", ok);
        }
        assert!(iban("DE298417365").is_none(), "a German VAT number is not an IBAN");
        assert!(iban("DE89 3704 0044 0532 0130 01").is_none());
        // a following group is cut off
        assert_eq!(iban("BE68 5390 0754 7034 ABCD").map(|r| r.0), Some(19));
    }

    #[test]
    fn dni_nie() {
        assert!(es_dni_nie("48291736Q").is_some());
        assert!(es_dni_nie("20481957C").is_some());
        assert!(es_dni_nie("X1234567L").is_some());
        assert!(es_dni_nie("12345678A").is_none());
    }
}
