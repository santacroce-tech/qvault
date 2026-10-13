//! Grouping entity variants: automatic merges for [`Level::Same`] pairs
//! (with cannot-link and conflict checks so nothing chains), plus
//! proposals for review from [`Level::Likely`] pairs and, when vectors are
//! available, from embedding similarity.
//!
//! Embeddings never merge anything on their own: they can only add a
//! proposal (similar vectors *and* similar strings) or withdraw a rule-based
//! proposal whose vectors disagree strongly. Everything works without them.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;

use super::forms::{self, Form, Level, Rule, Verdict};

/// One distinct string of one category.
#[derive(Debug, Clone)]
pub struct Item {
    pub category: String,
    pub text: String,
    pub form: Form,
    /// L2-normalised embedding of `text`, if computed with the current model.
    pub embedding: Option<Vec<f32>>,
    /// A group the user put this variant in (merge / split). Variants with
    /// the same pinned group stay together; different pinned groups are
    /// never merged automatically.
    pub pinned: Option<String>,
}

impl Item {
    pub fn new(category: &str, text: &str) -> Self {
        Item {
            category: category.to_string(),
            text: text.to_string(),
            form: forms::analyse(category, text),
            embedding: None,
            pinned: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ResolveConfig {
    /// Propose a merge when cosine ≥ this and the strings are similar enough.
    /// `None`: embeddings never add proposals.
    pub embed_propose: Option<f32>,
    /// …and the whole-string Jaro-Winkler similarity is at least this.
    pub embed_min_string: f32,
    /// Withdraw a rule-based proposal whose cosine is below this. `None`:
    /// never withdraw.
    pub embed_veto: Option<f32>,
    /// Turn a rule conflict into a proposal when the cosine is at least this
    /// (nicknames: "Bob Feeney" / "Robert Feeney"). `None`: conflicts stand.
    pub embed_override_conflict: Option<f32>,
    /// Drop (instead of flagging) ambiguous weak proposals: an initial or a
    /// surname alone that matches two or more groups that are different
    /// people ("Ms. Santos" with both "Maria Santos" and "João Santos").
    pub suppress_ambiguous: bool,
}

impl Default for ResolveConfig {
    fn default() -> Self {
        // Chosen on the entity-resolution eval (docs/qvac-integration.md §12).
        ResolveConfig { embed_propose: Some(0.90), embed_min_string: 0.80, ..ResolveConfig::rules_only() }
    }
}

impl ResolveConfig {
    pub fn rules_only() -> Self {
        ResolveConfig { embed_propose: None, embed_min_string: 1.0, embed_veto: None, embed_override_conflict: None, suppress_ambiguous: false }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProposalIdx {
    /// Representative items (one in each group) whose comparison produced it.
    pub a: usize,
    pub b: usize,
    pub rule: Rule,
    pub score: f32,
    pub cosine: Option<f32>,
    /// A weak link (initial / surname only) whose short form also matches
    /// another group that is a different person: the reviewer must decide
    /// which one, if any, it refers to.
    pub ambiguous: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Resolution {
    /// Cluster index for every item (0..clusters).
    pub cluster: Vec<usize>,
    pub clusters: usize,
    /// Automatic merges applied: (item, item, rule).
    pub merges: Vec<(usize, usize, Rule)>,
    /// One proposal per pair of clusters, best explanation first.
    pub proposals: Vec<ProposalIdx>,
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na.sqrt() * nb.sqrt())
    }
}

struct Dsu {
    parent: Vec<usize>,
    members: Vec<Vec<usize>>,
}

impl Dsu {
    fn new(n: usize) -> Self {
        Dsu { parent: (0..n).collect(), members: (0..n).map(|i| vec![i]).collect() }
    }
    fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.parent[r] != r {
            r = self.parent[r];
        }
        let mut c = x;
        while self.parent[c] != r {
            let n = self.parent[c];
            self.parent[c] = r;
            c = n;
        }
        r
    }
    fn union(&mut self, a: usize, b: usize) -> usize {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return ra;
        }
        let (big, small) = if self.members[ra].len() >= self.members[rb].len() { (ra, rb) } else { (rb, ra) };
        self.parent[small] = big;
        let moved = std::mem::take(&mut self.members[small]);
        self.members[big].extend(moved);
        big
    }
}

/// Block keys: pairs are only compared when they share one. Names,
/// companies and addresses block on full tokens (≥ 3 letters, or digits);
/// everything else on its identity key.
fn block_keys(f: &Form) -> Vec<String> {
    match f.kind {
        forms::Kind::Person | forms::Kind::Company | forms::Kind::Address => {
            let mut k: Vec<String> = f
                .toks
                .iter()
                .filter(|t| !t.initial && (t.s.chars().count() >= 3 || t.s.chars().any(|c| c.is_ascii_digit())))
                .map(|t| t.s.clone())
                .collect();
            // Acronyms and short company names still meet their long form
            // through the first letter of each token.
            if f.kind == forms::Kind::Company {
                let initials: String = f.toks.iter().filter_map(|t| t.s.chars().next()).collect();
                if initials.chars().count() >= 2 {
                    k.push(format!("#acr:{}", initials));
                }
                if f.toks.len() == 1 {
                    k.push(format!("#acr:{}", f.toks[0].s));
                }
            }
            if k.is_empty() {
                k.push(format!("#key:{}", f.key));
            }
            k
        }
        forms::Kind::Phone if f.key.len() >= 8 => vec![format!("#tail:{}", &f.key[f.key.len() - 8..])],
        _ => vec![format!("#key:{}", f.key)],
    }
}

/// Group `items`. `cannot` holds unordered index pairs the user separated.
pub fn resolve(items: &[Item], cannot: &HashSet<(usize, usize)>, cfg: &ResolveConfig) -> Resolution {
    let n = items.len();
    let mut verdicts: HashMap<(usize, usize), Verdict> = HashMap::new();
    let override_conflict = cfg.embed_override_conflict;
    let mut verdict = |i: usize, j: usize| -> Verdict {
        let k = (i.min(j), i.max(j));
        *verdicts.entry(k).or_insert_with(|| {
            if items[i].category != items[j].category {
                return Verdict { level: Level::Conflict, rule: Rule::None, score: 0.0 };
            }
            let v = forms::compare(&items[i].form, &items[j].form);
            if let (Level::Conflict, Some(t), Some(a), Some(b)) = (v.level, override_conflict, &items[i].embedding, &items[j].embedding) {
                let c = cosine(a, b);
                if c >= t {
                    return Verdict { level: Level::Likely, rule: Rule::Embedding, score: c };
                }
            }
            v
        })
    };
    let banned = |i: usize, j: usize| cannot.contains(&(i.min(j), i.max(j)));

    // Candidate pairs per category via blocking.
    let mut by_cat: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, it) in items.iter().enumerate() {
        by_cat.entry(it.category.as_str()).or_default().push(i);
    }
    let mut pairs: HashSet<(usize, usize)> = HashSet::new();
    let mut cos: HashMap<(usize, usize), f32> = HashMap::new();
    for idxs in by_cat.values() {
        let mut index: HashMap<String, Vec<usize>> = HashMap::new();
        for &i in idxs {
            for k in block_keys(&items[i].form) {
                index.entry(k).or_default().push(i);
            }
        }
        for list in index.values() {
            for x in 0..list.len() {
                for y in x + 1..list.len() {
                    let (a, b) = (list[x].min(list[y]), list[x].max(list[y]));
                    if a != b {
                        pairs.insert((a, b));
                    }
                }
            }
        }
        // Embedding neighbours (brute force; categories are small).
        if cfg.embed_propose.is_some() || cfg.embed_veto.is_some() {
            let with: Vec<usize> = idxs.iter().copied().filter(|&i| items[i].embedding.is_some()).collect();
            for x in 0..with.len() {
                for y in x + 1..with.len() {
                    let (a, b) = (with[x].min(with[y]), with[x].max(with[y]));
                    let c = cosine(items[a].embedding.as_ref().unwrap(), items[b].embedding.as_ref().unwrap());
                    cos.insert((a, b), c);
                    if cfg.embed_propose.map(|t| c >= t).unwrap_or(false) {
                        pairs.insert((a, b));
                    }
                }
            }
        }
    }
    let mut pairs: Vec<(usize, usize)> = pairs.into_iter().collect();
    pairs.sort();

    let mut dsu = Dsu::new(n);
    // Pinned groups first.
    let mut pinned_first: HashMap<&str, usize> = HashMap::new();
    for (i, it) in items.iter().enumerate() {
        if let Some(g) = it.pinned.as_deref() {
            match pinned_first.get(g) {
                Some(&f) => {
                    dsu.union(f, i);
                }
                None => {
                    pinned_first.insert(g, i);
                }
            }
        }
    }

    // Automatic merges, strongest rules first.
    let mut same: Vec<(usize, usize, Verdict)> = Vec::new();
    for &(a, b) in &pairs {
        let v = verdict(a, b);
        if v.level == Level::Same && !banned(a, b) {
            same.push((a, b, v));
        }
    }
    same.sort_by_key(|(a, b, v)| (v.rule.strength(), *a, *b));
    let mut merges = Vec::new();
    for (a, b, v) in same {
        let (ra, rb) = (dsu.find(a), dsu.find(b));
        if ra == rb {
            continue;
        }
        if can_merge(&dsu.members[ra], &dsu.members[rb], items, &banned, &mut verdict) {
            dsu.union(a, b);
            merges.push((a, b, v.rule));
        }
    }

    // Proposals between the resulting clusters.
    let mut best: HashMap<(usize, usize), ProposalIdx> = HashMap::new();
    for &(a, b) in &pairs {
        let (ra, rb) = (dsu.find(a), dsu.find(b));
        if ra == rb {
            continue;
        }
        let v = verdict(a, b);
        let c = cos.get(&(a, b)).copied();
        let rule_score = match v.level {
            // A Same pair that could not be merged (a conflict elsewhere in
            // the groups) is not proposed either: the conflict check below
            // rejects it.
            Level::Same | Level::Likely => Some((v.rule, v.score)),
            Level::Unrelated => match (cfg.embed_propose, c) {
                (Some(t), Some(cv)) if cv >= t && forms::string_similarity(&items[a].form, &items[b].form) >= cfg.embed_min_string => {
                    Some((Rule::Embedding, cv))
                }
                _ => None,
            },
            Level::Conflict => None,
        };
        let Some((rule, score)) = rule_score else { continue };
        if rule != Rule::Embedding {
            if let (Some(veto), Some(cv)) = (cfg.embed_veto, c) {
                if cv < veto {
                    continue;
                }
            }
        }
        let key = (ra.min(rb), ra.max(rb));
        let cand = ProposalIdx { a, b, rule, score, cosine: c, ambiguous: false };
        match best.get(&key) {
            Some(p) if (p.rule.strength(), -(p.score * 1000.0) as i32) <= (rule.strength(), -(score * 1000.0) as i32) => {}
            _ => {
                best.insert(key, cand);
            }
        }
    }
    let mut proposals: Vec<ProposalIdx> = Vec::new();
    let mut kept: Vec<(usize, usize)> = Vec::new();
    let mut keys: Vec<(usize, usize)> = best.keys().copied().collect();
    keys.sort();
    for key in keys {
        let p = best.remove(&key).unwrap();
        let (ra, rb) = key;
        let (ma, mb) = (dsu.members[ra].clone(), dsu.members[rb].clone());
        if !can_merge(&ma, &mb, items, &banned, &mut verdict) {
            continue;
        }
        proposals.push(p);
        kept.push(key);
    }
    {
        // For each group, the groups it is weakly linked to.
        let weak = |r: Rule| matches!(r, Rule::Initial | Rule::SurnameOnly);
        let mut links: HashMap<usize, Vec<usize>> = HashMap::new();
        for (p, &(ra, rb)) in proposals.iter().zip(kept.iter()) {
            if weak(p.rule) {
                links.entry(ra).or_default().push(rb);
                links.entry(rb).or_default().push(ra);
            }
        }
        let mut ambiguous: HashSet<usize> = HashSet::new();
        for (&g, others) in &links {
            'outer: for x in 0..others.len() {
                for y in x + 1..others.len() {
                    let (mx, my) = (dsu.members[others[x]].clone(), dsu.members[others[y]].clone());
                    let differ = mx.iter().any(|&a| my.iter().any(|&b| verdict(a, b).level == Level::Conflict));
                    if differ {
                        ambiguous.insert(g);
                        break 'outer;
                    }
                }
            }
        }
        // Flag (or drop) a weak proposal when its shorter side is ambiguous.
        let mut out = Vec::new();
        for (mut p, (ra, rb)) in proposals.into_iter().zip(kept.into_iter()) {
            if weak(p.rule) {
                let short = if dsu.members[ra].len() <= dsu.members[rb].len() && ambiguous.contains(&ra) {
                    Some(ra)
                } else if ambiguous.contains(&rb) {
                    Some(rb)
                } else if ambiguous.contains(&ra) {
                    Some(ra)
                } else {
                    None
                };
                // Only the side that is itself the weak form (fewest full names) counts.
                if let Some(s) = short {
                    let other = if s == ra { rb } else { ra };
                    let fulls = |r: usize| dsu.members[r].iter().map(|&i| items[i].form.full_tokens().count()).max().unwrap_or(0);
                    if fulls(s) <= fulls(other) {
                        if cfg.suppress_ambiguous {
                            continue;
                        }
                        p.ambiguous = true;
                    }
                }
            }
            out.push(p);
        }
        proposals = out;
    }
    proposals.sort_by(|x, y| x.rule.strength().cmp(&y.rule.strength()).then(y.score.partial_cmp(&x.score).unwrap_or(std::cmp::Ordering::Equal)));

    // Dense cluster numbering in item order.
    let mut cluster = vec![usize::MAX; n];
    let mut next = 0usize;
    let mut root_id: HashMap<usize, usize> = HashMap::new();
    for i in 0..n {
        let r = dsu.find(i);
        let id = *root_id.entry(r).or_insert_with(|| {
            next += 1;
            next - 1
        });
        cluster[i] = id;
    }
    Resolution { cluster, clusters: next, merges, proposals }
}

fn can_merge(
    ma: &[usize],
    mb: &[usize],
    items: &[Item],
    banned: &dyn Fn(usize, usize) -> bool,
    verdict: &mut dyn FnMut(usize, usize) -> Verdict,
) -> bool {
    let pa: HashSet<&str> = ma.iter().filter_map(|&i| items[i].pinned.as_deref()).collect();
    let pb: HashSet<&str> = mb.iter().filter_map(|&i| items[i].pinned.as_deref()).collect();
    if !pa.is_empty() && !pb.is_empty() && pa.is_disjoint(&pb) {
        return false;
    }
    for &x in ma {
        for &y in mb {
            if banned(x, y) || verdict(x, y).blocks_auto() {
                return false;
            }
        }
    }
    true
}

/// The most complete variant names the group: no initials, most full
/// tokens, "First Last" order, then most occurrences.
pub fn canonical_index(members: &[usize], items: &[Item], occurrences: &dyn Fn(usize) -> usize) -> usize {
    *members
        .iter()
        .max_by_key(|&&i| {
            let f = &items[i].form;
            (
                !f.has_initials(),
                f.full_tokens().count(),
                f.legal.is_some(),
                !f.reordered,
                !f.honorific,
                occurrences(i),
                std::cmp::Reverse(items[i].text.clone()),
            )
        })
        .expect("non-empty group")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(list: &[(&str, &str)]) -> Vec<Item> {
        list.iter().map(|(c, t)| Item::new(c, t)).collect()
    }

    fn groups(r: &Resolution, items: &[Item]) -> Vec<Vec<String>> {
        let mut g: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for (i, c) in r.cluster.iter().enumerate() {
            g.entry(*c).or_default().push(items[i].text.clone());
        }
        let mut v: Vec<Vec<String>> = g.into_values().collect();
        v.sort();
        v
    }

    #[test]
    fn merges_formatting_variants_and_proposes_the_rest() {
        let it = items(&[
            ("PersonName", "Maria Santos"),
            ("PersonName", "SANTOS, Maria"),
            ("PersonName", "Maria Fernanda Santos"),
            ("PersonName", "M. Santos"),
            ("PersonName", "Ms. Santos"),
            ("PersonName", "João Santos"),
            ("CompanyName", "Santos"),
        ]);
        let r = resolve(&it, &HashSet::new(), &ResolveConfig::rules_only());
        let g = groups(&r, &it);
        assert!(g.contains(&vec!["Maria Santos".to_string(), "SANTOS, Maria".into()]), "{:?}", g);
        assert!(g.contains(&vec!["Maria Fernanda Santos".to_string()]), "a two-name extension is proposed: {:?}", g);
        // Initial / surname-only variants stay separate (proposed), and the
        // company named Santos is never mixed with people.
        assert!(g.contains(&vec!["M. Santos".to_string()]));
        assert!(g.contains(&vec!["Ms. Santos".to_string()]));
        assert!(g.contains(&vec!["Santos".to_string()]));
        let maria = r.cluster[0];
        let props: Vec<(Rule, usize, usize)> =
            r.proposals.iter().map(|p| (p.rule, r.cluster[p.a].min(r.cluster[p.b]), r.cluster[p.a].max(r.cluster[p.b]))).collect();
        assert!(props.iter().any(|(rule, a, b)| *rule == Rule::Initial && (*a == maria || *b == maria)), "{:?}", props);
        assert!(props.iter().any(|(rule, _, _)| *rule == Rule::SurnameOnly));
        // João conflicts with Maria: never proposed together.
        let joao = r.cluster[5];
        assert!(!props.iter().any(|(_, a, b)| *a == maria.min(joao) && *b == maria.max(joao)));
        // Nothing crosses categories.
        let company = r.cluster[6];
        assert!(!props.iter().any(|(_, a, b)| *a == company || *b == company));
    }

    #[test]
    fn short_names_do_not_chain_different_people() {
        // "Ana Paula Ribeiro" extends to both, but the two long names conflict:
        // it joins one of them automatically and is never chained to the other.
        let it = items(&[
            ("PersonName", "Ana Paula da Conceição Ribeiro"),
            ("PersonName", "Ana Paula Ribeiro"),
            ("PersonName", "Ana Paula Lima Ribeiro"),
        ]);
        let r = resolve(&it, &HashSet::new(), &ResolveConfig::rules_only());
        assert_ne!(r.cluster[0], r.cluster[2]);
        assert_eq!(r.clusters, 2);
        assert!(r.proposals.is_empty(), "the other long name conflicts with the merged group");
    }

    #[test]
    fn user_constraints_win() {
        let mut it = items(&[("PersonName", "Maria Santos"), ("PersonName", "Santos, Maria"), ("PersonName", "M. Santos")]);
        // The user split "Santos, Maria" off and merged "M. Santos" in.
        it[0].pinned = Some("g1".into());
        it[2].pinned = Some("g1".into());
        it[1].pinned = Some("g2".into());
        let mut cannot = HashSet::new();
        cannot.insert((0, 1));
        let r = resolve(&it, &cannot, &ResolveConfig::rules_only());
        assert_eq!(r.cluster[0], r.cluster[2]);
        assert_ne!(r.cluster[0], r.cluster[1]);
        assert!(r.proposals.is_empty(), "a split pair is not proposed again");
    }

    #[test]
    fn embeddings_only_propose() {
        // Unrelated by the rules (no shared word order), similar vectors.
        let mut it = items(&[("CompanyName", "Grupo Albar"), ("CompanyName", "Albar Group"), ("CompanyName", "Banco Meridional"), ("CompanyName", "Banco Meridiano Sul")]);
        it[0].embedding = Some(vec![1.0, 0.0]);
        it[1].embedding = Some(vec![0.99, 0.141]);
        // A rule conflict (same first word, different rest) is never overridden.
        it[2].embedding = Some(vec![0.0, 1.0]);
        it[3].embedding = Some(vec![0.0, 1.0]);
        let cfg = ResolveConfig { embed_propose: Some(0.9), embed_min_string: 0.5, ..ResolveConfig::rules_only() };
        let r = resolve(&it, &HashSet::new(), &cfg);
        assert_eq!(r.clusters, 4, "never merged by embeddings");
        assert_eq!(r.proposals.len(), 1);
        assert_eq!(r.proposals[0].rule, Rule::Embedding);
        assert_eq!((r.proposals[0].a, r.proposals[0].b), (0, 1));
        let no = resolve(&it, &HashSet::new(), &ResolveConfig::rules_only());
        assert_eq!(no.clusters, 4);
        assert!(no.proposals.is_empty());
    }
}
