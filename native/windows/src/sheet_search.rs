//! A sheet's search (macos/SheetSearch.swift, pure so it is tested alone; the Mac's recall set,
//! native/mac/Tests/sheetsearch/queries.tsv, runs through it in `every_query_finds_its_row`).
//!
//! Each row is indexed once: its texts folded (case, accents, widths; − as -) and cut into words, each word also
//! squeezed to letters and digits (`x^{2}` is `x2`, `τ_{max}` is `τmax`) and, with Greek in it, in Latin letters
//! (`tmax`) and spelled out (`tau`, `taumax`), ∇ ∂ Σ ∫ √ by name too, all kept as ids into one vocabulary. Synonyms
//! (`ke` and `kinetic energy`) are added to a row's words where it has one of them. A query word is matched against
//! the vocabulary once (cached while it is typed on), as
//!   exact 10 · same stem 9 (`forces` is force, `rotating` rotation) · the start of a word 6 (typing, `deriv`) ·
//!   a stem's start 5 · inside a word 3 (`magnet`) · one letter off 2 · two off in a long word 1 (`intergral`)
//! (a synonym's a point less) times where the row has it: its name (note, tags) 4, its formula and explanation's
//! title 3, its sheet's, section's or group's title 2, its explanation's letters and related formulas 1. A row's
//! score is the sum over the query's words, plus a bonus when the words come together in its own text (`moment of
//! inertia`), when the query is a clause of its note or title (`average` is "mean, average") or ends or starts one
//! (`flux` is "heat flux", `ideal gas` "ideal gas law"), and when it is what the row's formula defines (`Pr`, `tau
//! max`, `tanh` for tanh(z) = …, `del f` for ∇f, ζ typed as itself). A row matches when it has every word but the
//! little ones (`of`, `the`); when none does, every word but one ("closest matches"). A formula typed with spaces
//! (`P = V I`) is first looked for whole in a row's formula; when some row has it, only those rows match.
//!
//! Built off the UI thread (sheets.rs `Catalog`), searched on it: a query is a few hundred microseconds.

use std::collections::{HashMap, HashSet};

use unicode_normalization::UnicodeNormalization;

struct Row {
    /// token ids in each of `WEIGHTS`' places, a word in the first place it is
    fields: [Vec<u32>; 4],
    /// and the synonyms of its words, in the same place, a point under the word itself
    synonyms: [Vec<u32>; 4],
    /// its formula's letters and digits (and in Latin letters), for a formula typed whole
    formula: Vec<Vec<u8>>,
    /// the words of its first two places joined by single spaces (after one), for a phrase's bonus
    own_words: Vec<u8>,
    /// its note's and explanation title's clauses (cut at , ; : · and brackets), each its words joined by spaces
    clauses: Vec<String>,
    /// what its formula defines, the letters left of its `=` (`pr` for Pr = μ c_p/k)
    defines: Vec<String>,
    /// its note's length in words (its explanation title's without one)
    note_length: usize,
    /// everything folded, for a query word that is only symbols (`∫`, `±`)
    hay: String,
}

/// What a word counts for in each place: a row's name (note, tags, the line it opens), its formula and explanation's
/// title, its sheet's, section's and group's titles, its explanation's letters and related formulas.
const WEIGHTS: [i32; 4] = [4, 3, 2, 1];

/// A tier's points: none, two letters off, one letter off, inside a word, a stem's start, a word's start, the same
/// stem, exact.
const POINTS: [i32; 8] = [0, 1, 2, 3, 5, 6, 9, 10];

/// What a row on the sheet that's open adds to its score, so a tie (`modulus` on complex numbers) goes its way.
pub const OPEN_SHEET_BONUS: i32 = 10;

/// The little words a query may leave out of a row.
const LITTLE: [&str; 14] = [
    "of", "the", "a", "an", "for", "in", "and", "to", "on", "with", "from", "by", "is", "at",
];

/// A search's scores: each row's, 0 where it doesn't match; `closest` when no row had every word.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    pub scores: Vec<i32>,
    pub closest: bool,
}

/// A sheet row's texts.
#[derive(Clone, Debug, Default)]
pub struct RowText {
    pub lead: Vec<String>,
    pub show: String,
    pub copy: String,
    pub note: String,
    pub lead_copy: Vec<String>,
    pub parts: Vec<String>,
    pub open: String,
    /// words it is found by but doesn't show (the generator's `tags`)
    pub tags: String,
    pub explain_title: String,
    /// the explanation's letters (symbol, meaning, unit) and related formulas (formula, when)
    pub vars: Vec<[String; 3]>,
    pub related: Vec<[String; 2]>,
}

impl RowText {
    /// A row's texts: its runs' words (no `^{`), what it copies and opens, its tags and its explanation.
    pub fn of(item: &qcalc_core::app::identities::Identity) -> RowText {
        let plain = |s: &str| -> String {
            qcalc_core::app::identities::identity_runs(s)
                .into_iter()
                .map(|r| r.text)
                .collect()
        };
        let mut r = RowText {
            lead: item.lead.iter().flatten().map(|l| plain(l)).collect(),
            show: plain(&item.show),
            copy: item.copy.clone(),
            note: item.note.clone().unwrap_or_default(),
            lead_copy: item.lead_copy.clone().unwrap_or_default(),
            parts: item
                .parts
                .iter()
                .flatten()
                .map(|p| p.copy.clone())
                .collect(),
            open: item.open.clone().unwrap_or_default(),
            tags: item.tags.clone().unwrap_or_default(),
            ..RowText::default()
        };
        if let Some(e) = &item.explain {
            r.explain_title = e.title.clone();
            r.vars = e.vars.clone();
            r.related = e.related.clone();
        }
        r
    }
}

#[derive(Default)]
pub struct Search {
    rows: Vec<Row>,
    vocab: Vec<String>,
    ids: HashMap<String, u32>,
    stems: HashMap<String, Vec<u32>>,
    text_cache: HashMap<String, Vec<u32>>,
    word_cache: HashMap<String, Vec<u8>>,
    fold_cache: HashMap<String, String>,
}

impl Search {
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Lets go of what only indexing needs (the per-text caches), once every row is in.
    pub fn done_adding(&mut self) {
        self.text_cache = HashMap::new();
        self.fold_cache = HashMap::new();
    }

    /// A row under its sheet's, section's and group's titles.
    pub fn add(&mut self, r: &RowText, titles: [&str; 3]) {
        let mut more: Vec<String> = r.related.iter().map(|x| x.join(" ")).collect();
        for v in &r.vars {
            more.push(v[1..].join(" "));
            if let Some(name) = unit_names(v[2].trim()) {
                more.push(name.to_string());
            }
        }
        let mut formula: Vec<String> = r.lead.clone();
        formula.push(r.show.clone());
        formula.push(r.copy.clone());
        formula.extend(r.lead_copy.iter().cloned());
        formula.extend(r.parts.iter().cloned());
        let mut second = formula.clone();
        second.push(r.explain_title.clone());
        let places = [
            vec![r.note.clone(), r.tags.clone(), r.open.clone()],
            second,
            titles.iter().map(|t| t.to_string()).collect(),
            more,
        ];
        let names = [r.note.clone(), r.explain_title.clone()];
        self.index(&places, &formula, &names);
    }

    fn index(&mut self, places: &[Vec<String>; 4], formula: &[String], names: &[String]) {
        let folded: Vec<Vec<String>> = places
            .iter()
            .map(|p| p.iter().map(|t| self.folded(t)).collect())
            .collect();
        let fields: Vec<HashSet<u32>> = places
            .iter()
            .map(|p| p.iter().flat_map(|t| self.tokens(t)).collect())
            .collect();
        let mut synonyms: Vec<HashSet<u32>> = vec![HashSet::new(); 4];
        // synonyms where the row has one
        for (n, texts) in folded.iter().enumerate() {
            let words = words(&texts.join(" "));
            let have: HashSet<&str> = words.iter().map(String::as_str).collect();
            let padded = format!(" {} ", words.join(" "));
            for group in SYNONYMS {
                let found = group.iter().any(|phrase| {
                    let w: Vec<&str> = phrase.split(' ').collect();
                    w.iter().all(|x| have.contains(x))
                        && (w.len() == 1 || padded.contains(&format!(" {phrase} ")))
                });
                if found {
                    for word in group.iter().flat_map(|p| p.split(' ')) {
                        let id = self.id(word);
                        synonyms[n].insert(id);
                    }
                }
            }
        }
        let mut seen: HashSet<u32> = HashSet::new();
        let mut unique: [Vec<u32>; 4] = Default::default();
        for (n, field) in fields.iter().enumerate() {
            let mut out: Vec<u32> = field.difference(&seen).copied().collect();
            out.sort_unstable();
            unique[n] = out;
            seen.extend(field);
        }
        let mut similar: [Vec<u32>; 4] = Default::default();
        for (n, syn) in synonyms.iter().enumerate() {
            let mut out: Vec<u32> = syn
                .iter()
                .filter(|id| !fields[n].contains(id) && !seen.contains(id))
                .copied()
                .collect();
            out.sort_unstable();
            similar[n] = out;
        }
        let squeezed: Vec<String> = formula.iter().map(|f| compact(&fold(f))).collect();
        let mut keys = squeezed.clone();
        for s in &squeezed {
            let l = latin(s);
            if !squeezed.contains(&l) {
                keys.push(l);
            }
        }
        let first_two: Vec<String> = folded[..2].iter().flatten().cloned().collect();
        let own_words = format!(" {}", words(&first_two.join(" ")).join(" ")).into_bytes();
        let mut clauses: Vec<String> = Vec::new();
        for name in names {
            // an aside in brackets isn't a clause of its own ("Mean (hydrostatic) pressure" is "mean pressure")
            let f = self.folded(name);
            let bare = if f.contains('(') { drop_asides(&f) } else { f };
            for clause in bare.split([',', ';', ':', '·', '—', '–']) {
                let w = words(clause).join(" ");
                if !w.is_empty() && !clauses.contains(&w) {
                    clauses.push(w);
                }
            }
        }
        let mut defines: Vec<String> = Vec::new();
        for f in formula {
            let Some(eq) = f.find('=') else { continue };
            if eq == 0 {
                continue;
            }
            let mut side = fold(&f[..eq]).trim().to_string();
            // a function defined by its argument is its name (`tanh(z) = …` is tanh)
            if side.ends_with(')')
                && let Some(open) = side.find('(')
                && open > 0
                && side[..open].chars().all(char::is_alphabetic)
            {
                side.truncate(open);
            }
            let left = compact(&side);
            let count = left.chars().count();
            let mut keys: Vec<String> = Vec::new();
            // a lone letter (F, β) is on the left of too many rows to say much, unless it's Greek typed as itself
            if count == 1 && left.chars().all(|c| greek_name(c).is_some()) {
                keys.push(left.clone());
            }
            if (2..=6).contains(&count) {
                keys.push(left.clone());
                keys.push(compact(&latin(&left)));
                keys.push(spelled(&left));
            }
            // ∇f is `del f` and `nabla f` too (only a symbol before letters: not ∇ × F)
            for (symbol, names) in SYMBOL_NAMES {
                if !side.contains(*symbol) {
                    continue;
                }
                let rest: String = side.chars().filter(|c| c != symbol).collect();
                if rest.is_empty() || !rest.chars().all(char::is_alphabetic) {
                    continue;
                }
                keys.extend(names.iter().map(|n| format!("{n}{rest}")));
            }
            for k in keys {
                if !k.is_empty() && !defines.contains(&k) {
                    defines.push(k);
                }
            }
        }
        let note_length = names
            .iter()
            .map(|n| words(&self.folded(n)).len())
            .find(|&n| n > 0)
            .unwrap_or(99);
        let hay = folded
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        self.rows.push(Row {
            fields: unique,
            synonyms: similar,
            formula: keys.into_iter().map(String::into_bytes).collect(),
            own_words,
            clauses,
            defines,
            note_length,
            hay,
        });
        self.word_cache.clear();
    }

    pub fn search(&mut self, query: &str) -> Outcome {
        let words_typed: Vec<String> = fold(query).split_whitespace().map(str::to_string).collect();
        let mut out = Outcome {
            scores: vec![0; self.rows.len()],
            closest: false,
        };
        if words_typed.is_empty() {
            return out;
        }
        // a formula typed with spaces (`P = V I`): its words alone match nearly every row (`=`, `v` and `i` are
        // everywhere); only with a symbol in it (`z transform` and `a cos b sin` are words, found by name)
        let formula_like = words_typed.len() > 1
            && query.chars().any(|c| {
                !(c.is_alphabetic()
                    || c.is_numeric()
                    || c.is_whitespace()
                    || c == '\''
                    || c == '\u{2019}')
            });
        let phrase = if formula_like {
            compact(&words_typed.concat())
        } else {
            String::new()
        };
        if phrase.chars().count() > 1 {
            let p = phrase.as_bytes();
            for (n, row) in self.rows.iter().enumerate() {
                if row.formula.iter().any(|f| contains(f, p)) {
                    out.scores[n] = 100
                        + if row.formula.iter().any(|f| f == p) {
                            20
                        } else {
                            0
                        };
                }
            }
            if out.scores.iter().any(|&s| s > 0) {
                return out;
            }
        }
        let little = |w: &str| LITTLE.contains(&w);
        let needed = words_typed.iter().filter(|w| !little(w)).count();
        let need = if needed == 0 {
            words_typed.len()
        } else {
            needed
        };
        let tiers: Vec<Vec<u8>> = words_typed.iter().map(|w| self.tiers(w)).collect();
        let asked = words(&words_typed.join(" ")).join(" ");
        let asked_length = asked.split(' ').filter(|w| !w.is_empty()).count();
        // a symbol typed alone, or spelled in parts (`tau max`, `omega n`)
        let symbol = if words_typed.len() <= 2 {
            compact(&words_typed.concat())
        } else {
            String::new()
        };
        let together = if words_typed.len() > 1 {
            format!(" {}", words(&words_typed.join(" ")).join(" ")).into_bytes()
        } else {
            Vec::new()
        };
        let mut had = vec![0usize; self.rows.len()];
        for (n, row) in self.rows.iter().enumerate() {
            let (mut score, mut have, mut in_own) = (0i32, 0usize, 0usize);
            for (w, word) in words_typed.iter().enumerate() {
                let t = &tiers[w];
                let mut best = 0;
                if t.is_empty() {
                    // only symbols: anywhere in the row
                    if row.hay.contains(word.as_str()) {
                        best = 3 * POINTS[6];
                    }
                } else {
                    let mut own = false;
                    for (f, weight) in WEIGHTS.iter().enumerate() {
                        let tier = best_of(t, &row.fields[f]);
                        let similar = best_of(t, &row.synonyms[f]);
                        if (tier > 0 || similar > 0) && f < 2 {
                            own = true;
                        }
                        best = best
                            .max(weight * POINTS[tier])
                            .max(weight * (POINTS[similar] - 1));
                    }
                    if own {
                        in_own += 1;
                    }
                }
                if best > 0 {
                    score += best;
                    if needed == 0 || !little(word) {
                        have += 1;
                    }
                }
            }
            if score == 0 {
                continue;
            }
            if in_own == words_typed.len() {
                score += 6;
            }
            if !together.is_empty() && contains(&row.own_words, &together) {
                score += 15;
            }
            if !asked.is_empty() {
                if row.clauses.contains(&asked) {
                    score += 12 + (3 - (row.note_length as i32 - asked_length as i32)).max(0);
                } else if row.clauses.iter().any(|c| {
                    c.ends_with(&format!(" {asked}")) || c.starts_with(&format!("{asked} "))
                }) {
                    score += 5;
                }
            }
            if !symbol.is_empty() && row.defines.contains(&symbol) {
                score += 25;
            }
            had[n] = have;
            out.scores[n] = score;
        }
        if had.iter().any(|&h| h >= need) {
            for (n, h) in had.iter().enumerate() {
                if *h < need {
                    out.scores[n] = 0;
                }
            }
        } else if need > 1 && had.iter().any(|&h| h + 1 >= need) {
            out.closest = true;
            for (n, h) in had.iter().enumerate() {
                if *h + 1 < need {
                    out.scores[n] = 0;
                }
            }
        } else {
            out.scores.iter_mut().for_each(|s| *s = 0);
        }
        out
    }

    /// The rows a search matched, best first: by score (and `bonus`, the open sheet's), then in the order they were
    /// added (a sheet's own order: its usual form first).
    pub fn ranked(outcome: &Outcome, bonus: impl Fn(usize) -> i32) -> Vec<usize> {
        let mut keyed: Vec<(usize, i32)> = outcome
            .scores
            .iter()
            .enumerate()
            .filter(|(_, s)| **s > 0)
            .map(|(n, s)| (n, s + bonus(n)))
            .collect();
        keyed.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        keyed.into_iter().map(|k| k.0).collect()
    }

    /// Each vocabulary word's tier for one query word (empty for a word that is only symbols).
    fn tiers(&mut self, word: &str) -> Vec<u8> {
        if let Some(cached) = self.word_cache.get(word) {
            return cached.clone();
        }
        let key = compact(word);
        if key.is_empty() {
            return Vec::new();
        }
        let mut out = vec![0u8; self.vocab.len()];
        let k = key.as_bytes();
        let stem = stem(&key);
        let s = stem.as_bytes();
        // a word's start, inside one, a stem's start
        for (n, v) in self.vocab.iter().enumerate() {
            let v = v.as_bytes();
            if v.len() >= k.len() && k.len() >= 2 && v.starts_with(k) {
                out[n] = 5;
            } else if s.len() >= 4 && v.len() > s.len() && v.starts_with(s) {
                out[n] = 4;
            } else if k.len() >= 3 && v.len() > k.len() && contains(v, k) {
                out[n] = 3;
            }
        }
        // a longer query word whose start is a word's stem (`integrals`, `ohms`)
        if k.len() >= 4 {
            for (n, word) in self.vocab.iter().enumerate() {
                let v = word.as_bytes();
                if out[n] < 4
                    && v.len() >= 3
                    && v.len() < k.len()
                    && k.len() - v.len() <= 3
                    && k.starts_with(v)
                {
                    let vs = self::stem(word);
                    let vs = vs.as_bytes();
                    if vs.len() >= 3
                        && k.starts_with(vs)
                        && std::str::from_utf8(&k[vs.len()..]).is_ok_and(|e| ENDINGS.contains(&e))
                    {
                        out[n] = 4;
                    }
                }
            }
        }
        // one letter off, or two in a long word, for words that are letters (not `x2`); one off over two
        // (`intergral` is integral before internal, `momentem` momentum before moment)
        if k.len() >= 4 && key.chars().all(char::is_alphabetic) {
            let most = if k.len() >= 8 { 2 } else { 1 };
            for (n, v) in self.vocab.iter().enumerate() {
                let v = v.as_bytes();
                if out[n] == 0 && v.len().abs_diff(k.len()) <= most && v.first() == k.first() {
                    let d = distance(k, v, most);
                    if d <= most {
                        out[n] = if d == 1 { 2 } else { 1 };
                    }
                }
            }
        }
        if stem.chars().count() >= 3
            && let Some(same) = self.stems.get(&stem)
        {
            for &id in same {
                if out[id as usize] < 6 {
                    out[id as usize] = 6;
                }
            }
        }
        if let Some(&exact) = self.ids.get(&key) {
            out[exact as usize] = 7;
        }
        // a long session types thousands of words, each a vocabulary's worth of bytes
        if self.word_cache.len() >= 256 {
            self.word_cache.clear();
        }
        self.word_cache.insert(word.to_string(), out.clone());
        out
    }

    fn id(&mut self, token: &str) -> u32 {
        if let Some(&id) = self.ids.get(token) {
            return id;
        }
        let id = self.vocab.len() as u32;
        self.vocab.push(token.to_string());
        self.ids.insert(token.to_string(), id);
        let stem = stem(token);
        if stem.chars().count() >= 3 {
            self.stems.entry(stem).or_default().push(id);
        }
        id
    }

    /// A text's words, each also squeezed to its letters and digits and in Latin letters where it has Greek.
    fn tokens(&mut self, text: &str) -> Vec<u32> {
        if let Some(cached) = self.text_cache.get(text) {
            return cached.clone();
        }
        let folded = self.folded(text);
        let mut found: HashSet<String> = words(&folded).into_iter().collect();
        for chunk in folded.split_whitespace() {
            let c = compact(chunk);
            if !c.is_empty() {
                found.insert(c);
            }
        }
        // a size written 2 × 2 is also 2x2
        if folded.contains('×') {
            found.extend(matrix_sizes(&folded));
        }
        let snapshot: Vec<String> = found.iter().cloned().collect();
        for t in &snapshot {
            if !t.is_ascii() {
                let l = compact(&latin(t));
                if !l.is_empty() {
                    found.insert(l);
                }
            }
        }
        // and with Greek spelled out, as it's typed on a keyboard: τ_max is `tau` `max` and `taumax`
        let snapshot: Vec<String> = found.iter().cloned().collect();
        for t in &snapshot {
            if t.chars()
                .any(|c| c as u32 >= 0xB5 && greek_name(c).is_some())
            {
                found.insert(spelled(t));
            }
        }
        // and the symbols with names of their own
        for c in text.chars().filter(|c| !c.is_ascii()) {
            if let Some((_, names)) = SYMBOL_NAMES.iter().find(|(s, _)| *s == c) {
                found.extend(names.iter().map(|n| n.to_string()));
            }
        }
        let mut sorted: Vec<String> = found.into_iter().collect();
        sorted.sort_unstable();
        let out: Vec<u32> = sorted.iter().map(|t| self.id(t)).collect();
        self.text_cache.insert(text.to_string(), out.clone());
        out
    }

    /// A sheet repeats its titles and letters on row after row: each folded once.
    fn folded(&mut self, text: &str) -> String {
        if let Some(f) = self.fold_cache.get(text) {
            return f.clone();
        }
        let f = fold(text);
        self.fold_cache.insert(text.to_string(), f.clone());
        f
    }
}

fn best_of(tiers: &[u8], ids: &[u32]) -> usize {
    let mut best = 0u8;
    for &id in ids {
        let t = tiers[id as usize];
        if t > best {
            best = t;
            if best == 7 {
                break;
            }
        }
    }
    usize::from(best)
}

/// The SI units the explanations give, by name and by what they measure, so a word finds the symbol (`ohm` finds
/// rows in Ω, `newton` and `force` rows in N).
fn unit_names(unit: &str) -> Option<&'static str> {
    Some(match unit {
        "Ω" => "ohm ohms resistance resistor",
        "S" => "siemens conductance",
        "V" => "volt volts voltage potential emf",
        "A" => "amp amps ampere current",
        "W" => "watt watts power",
        "J" => "joule joules energy work heat",
        "N" => "newton newtons force",
        "N·m" => "newton meter torque moment",
        "Pa" => "pascal pressure stress",
        "C" => "coulomb charge",
        "F" => "farad capacitance capacitor",
        "H" => "henry inductance inductor",
        "T" => "tesla magnetic field flux density",
        "Wb" => "weber magnetic flux",
        "Hz" => "hertz frequency",
        "rad/s" => "angular frequency velocity",
        "kg" => "kilogram mass",
        "m" => "meter metre length distance",
        "s" => "second seconds time",
        "K" => "kelvin temperature",
        "°C" => "celsius temperature",
        "mol" => "mole moles amount",
        "m/s" => "velocity speed",
        "m/s^{2}" => "acceleration",
        "kg·m/s" => "momentum impulse",
        "m^{2}" => "area",
        "m^{3}" => "volume",
        "kg/m^{3}" => "density",
        "W/(m·K)" => "thermal conductivity",
        "N/m" => "spring constant stiffness",
        "kg·m^{2}" => "moment of inertia",
        "m^{4}" => "second moment of area",
        "rad" => "radian angle",
        "lm" => "lumen",
        "cd" => "candela",
        "V/m" => "electric field",
        "A/m" => "magnetic field strength",
        "Pa·s" => "viscosity",
        "m^{3}/s" => "flow rate",
        _ => return None,
    })
}

/// Words that mean the same, any one finding a row with another (each a phrase of folded words).
const SYNONYMS: &[&[&str]] = &[
    &["ke", "kinetic energy"],
    &["pe", "potential energy"],
    &["moi", "moment of inertia", "mass moment of inertia"],
    &["parallel axis", "shift theorem", "steiner"],
    &["ohm", "ohms law"],
    &["sohcahtoa", "soh cah toa"],
    &["derivative", "differentiate", "differentiation"],
    &["integral", "integrate", "integration", "antiderivative"],
    &["std", "stdev", "standard deviation", "sd"],
    &["var", "variance"],
    &["avg", "average", "mean"],
    &["kvl", "kirchhoffs voltage law"],
    &["kcl", "kirchhoffs current law"],
    &["emf", "electromotive force"],
    &["eom", "equations of motion", "equation of motion"],
    &["fbd", "free body diagram"],
    &["shm", "simple harmonic"],
    &["ode", "differential equation"],
    &["pde", "partial differential equation"],
    &["sho", "harmonic oscillator"],
    &[
        "mass centre",
        "center of mass",
        "centre of mass",
        "mass center",
        "com",
    ],
    &["torque", "moment of a force"],
    &["fft", "dft", "discrete fourier transform"],
    &["npv", "net present value"],
    &["log", "logarithm"],
    &["ln", "natural log"],
    &["sqrt", "square root"],
    &["temp", "temperature"],
    &["vel", "velocity"],
    &["accel", "acceleration"],
    &["freq", "frequency"],
    &["cos law", "law of cosines"],
    &["sin law", "law of sines"],
    &["hyp", "hypotenuse"],
    &["lhopital", "l hopital", "l hospital"],
    &["eigen", "eigenvalue", "eigenvector"],
    &["det", "determinant"],
    &["pmf", "probability mass"],
    &["pdf", "probability density"],
    &["cdf", "cumulative distribution"],
    &["rms", "root mean square"],
    &["sse", "sum of squares"],
    &["mse", "mean squared error"],
];

const SYMBOL_NAMES: &[(char, &[&str])] = &[
    ('∇', &["nabla", "del"]),
    ('∂', &["partial"]),
    ('Σ', &["sum"]),
    ('∑', &["sum"]),
    ('∏', &["product"]),
    ('∫', &["integral"]),
    ('√', &["sqrt"]),
];

/// What a longer query word may end with past a word's stem (`integrals`, `ohms`, `rotational`; not `momentem`).
const ENDINGS: &[&str] = &[
    "s", "es", "e", "ed", "er", "ers", "ing", "ings", "ly", "al", "ally", "ic", "ical", "ion",
    "ions", "ation", "ations", "ational", "ment", "ments", "ness", "ity", "ive", "ian", "y", "ies",
    "ial", "ous",
];

const SUFFIXES: [(&str, &str); 21] = [
    ("izations", "iz"),
    ("ization", "iz"),
    ("ational", "at"),
    ("ations", "at"),
    ("ation", "at"),
    ("ically", "ic"),
    ("ings", ""),
    ("ing", ""),
    ("ions", ""),
    ("ion", ""),
    ("ies", "y"),
    ("ied", "y"),
    ("ness", ""),
    ("ments", ""),
    ("ment", ""),
    ("ally", "al"),
    ("ers", ""),
    ("ed", ""),
    ("es", ""),
    ("ly", ""),
    ("s", ""),
];

/// Greek letters by name, lowercase (the text is folded first): α to ω with the final sigma, and the variants.
fn greek_name(c: char) -> Option<&'static str> {
    const NAMES: [&str; 25] = [
        "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
        "lambda", "mu", "nu", "xi", "omicron", "pi", "rho", "sigma", "sigma", "tau", "upsilon",
        "phi", "chi", "psi", "omega",
    ];
    match c {
        'α'..='ω' => NAMES.get(c as usize - 0x3B1).copied(),
        '\u{3D1}' => Some("theta"),
        '\u{3D5}' => Some("phi"),
        '\u{3F5}' => Some("epsilon"),
        '\u{B5}' => Some("mu"),
        '\u{3D6}' => Some("pi"),
        _ => None,
    }
}

/// Greek letters by name: `ωn` is `omegan`.
fn spelled(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        match greek_name(c) {
            Some(name) => out.push_str(name),
            None => out.push(c),
        }
    }
    out
}

/// Case, accents and width folded, `−` read as `-`.
pub fn fold(text: &str) -> String {
    text.nfd()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .map(|c| match c {
            '\u{2212}' => '-',
            '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0).unwrap_or(c),
            '\u{3000}' => ' ',
            c => c,
        })
        .flat_map(char::to_lowercase)
        .collect()
}

/// Letters and digits alone, so `tmax` finds τ_max's `t_max` and `x2` finds x^{2}.
pub fn compact(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// The words of folded text: runs of letters and digits (an apostrophe inside a word is dropped: `ohm's` is `ohms`).
pub fn words(folded: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    for c in folded.chars() {
        if c.is_alphanumeric() {
            word.push(c);
        } else if (c == '\'' || c == '\u{2019}') && !word.is_empty() {
            continue;
        } else if !word.is_empty() {
            out.push(std::mem::take(&mut word));
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// Greek in Latin letters, as ICU's Greek-Latin then folded: τ_max is t_max, θ is th, ω is o, Δ is d.
pub fn latin(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in fold(text).chars() {
        out.push_str(match c {
            'α' => "a",
            'β' => "b",
            'γ' => "g",
            'δ' => "d",
            'ε' | 'ϵ' | 'η' => "e",
            'ζ' => "z",
            'θ' | 'ϑ' => "th",
            'ι' => "i",
            'κ' => "k",
            'λ' => "l",
            'μ' => "m",
            'ν' => "n",
            'ξ' => "x",
            'ο' | 'ω' => "o",
            'π' | 'ϖ' => "p",
            'ρ' => "r",
            'σ' | 'ς' => "s",
            'τ' => "t",
            'υ' => "y",
            'φ' | 'ϕ' => "ph",
            'χ' => "ch",
            'ψ' => "ps",
            _ => {
                out.push(c);
                continue;
            }
        });
    }
    out
}

/// A light English stem: `forces` and `force` are `forc`, `rotating` and `rotation` `rotat`, `integrals` `integral`.
fn stem(word: &str) -> String {
    let count = word.chars().count();
    if count <= 3 || !word.chars().all(char::is_alphabetic) {
        return word.to_string();
    }
    let mut w = word.to_string();
    if let Some(s) = w.strip_suffix("'s") {
        w = s.to_string();
    }
    for (end, with) in SUFFIXES {
        if w.ends_with(end) && w.chars().count() - end.chars().count() >= 3 {
            w.truncate(w.len() - end.len());
            w.push_str(with);
            break;
        }
    }
    if w.ends_with('e') && w.chars().count() > 3 {
        w.pop();
    }
    w
}

/// "Mean (hydrostatic) pressure" is "mean pressure": each innermost bracket and the space before it go.
fn drop_asides(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        let after = &rest[open + 1..];
        match after.find([')', '(']) {
            Some(close) if after.as_bytes()[close] == b')' => {
                out.push_str(rest[..open].trim_end());
                rest = &after[close + 1..];
            }
            _ => {
                out.push_str(&rest[..=open]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `2 × 2` and `2×3` as `2x2`, `2x3`.
fn matrix_sizes(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        if c != '×' {
            continue;
        }
        let mut a = i;
        if a > 0 && chars[a - 1] == ' ' {
            a -= 1;
        }
        let a_end = a;
        while a > 0 && chars[a - 1].is_ascii_digit() {
            a -= 1;
        }
        let mut b = i + 1;
        if chars.get(b) == Some(&' ') {
            b += 1;
        }
        let b_start = b;
        while chars.get(b).is_some_and(char::is_ascii_digit) {
            b += 1;
        }
        if a < a_end && b > b_start {
            let left: String = chars[a..a_end].iter().collect();
            let right: String = chars[b_start..b].iter().collect();
            out.push(format!("{left}x{right}"));
        }
    }
    out
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    needle.len() <= hay.len() && hay.windows(needle.len()).any(|w| w == needle)
}

/// Edits (with a swap of two letters as one) between two words, or more than `most` once it's sure to be.
fn distance(a: &[u8], b: &[u8], most: usize) -> usize {
    let (n, m) = (a.len(), b.len());
    let mut before = vec![0usize; m + 1];
    let mut previous: Vec<usize> = (0..=m).collect();
    let mut current = vec![0usize; m + 1];
    for i in 1..=n {
        current[0] = i;
        let mut low = i;
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut d = (previous[j] + 1)
                .min(current[j - 1] + 1)
                .min(previous[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d = d.min(before[j - 2] + 1);
            }
            current[j] = d;
            low = low.min(d);
        }
        if low > most {
            return most + 1;
        }
        std::mem::swap(&mut before, &mut previous);
        std::mem::swap(&mut previous, &mut current);
    }
    previous[m]
}

#[cfg(test)]
mod tests {
    use super::*;
    use qcalc_core::app::identities;

    fn row(show: &str, note: &str) -> RowText {
        RowText {
            show: show.into(),
            copy: show.into(),
            note: note.into(),
            ..RowText::default()
        }
    }

    #[test]
    fn words_are_folded_squeezed_and_spelled() {
        assert_eq!(fold("Thévenin"), "thevenin");
        assert_eq!(fold("Ａb−c"), "ab-c");
        assert_eq!(words("ohm's law, 2x"), vec!["ohms", "law", "2x"]);
        assert_eq!(latin("τmax"), "tmax");
        assert_eq!(spelled("ωn"), "omegan");
        assert_eq!(stem("forces"), "forc");
        assert_eq!(stem("rotating"), "rotat");
        assert_eq!(stem("rotation"), "rotat");
        assert_eq!(drop_asides("mean (hydrostatic) pressure"), "mean pressure");
        assert_eq!(matrix_sizes("a 2 × 3 matrix"), vec!["2x3"]);
        assert_eq!(distance(b"intergral", b"integral", 2), 1);
        assert_eq!(distance(b"abcd", b"abdc", 1), 1);
    }

    #[test]
    fn a_word_finds_its_row_and_greek_spelled_in_latin() {
        let mut s = Search::default();
        s.add(
            &row("τ_max = T r/J", "max shear stress"),
            ["mech", "Torsion", "Shafts"],
        );
        s.add(
            &row("σ_x = M y/I", "bending stress"),
            ["mech", "Beams", "Stress"],
        );
        s.add(
            &row("θ = T L/(G J)", "angle of twist"),
            ["mech", "Torsion", "Shafts"],
        );
        s.add(
            &row("F = m a", "Newton's second law"),
            ["physics", "Dynamics", "Laws"],
        );
        s.done_adding();
        let top = |s: &mut Search, q: &str| {
            let o = s.search(q);
            Search::ranked(&o, |_| 0).first().copied()
        };
        assert_eq!(top(&mut s, "tmax"), Some(0));
        assert_eq!(top(&mut s, "tau max"), Some(0));
        assert_eq!(top(&mut s, "sx"), Some(1));
        assert_eq!(top(&mut s, "twist"), Some(2));
        assert_eq!(top(&mut s, "theta"), Some(2));
        assert_eq!(top(&mut s, "newtons second law"), Some(3));
        // a typo
        assert_eq!(top(&mut s, "bendng"), Some(1));
        // a formula typed whole
        assert_eq!(top(&mut s, "F = m a"), Some(3));
        // a typed τ finds only τ
        let o = s.search("τ");
        assert!(o.scores[0] > 0 && o.scores[1] == 0 && o.scores[3] == 0);
        // closest matches when nothing has every word
        let o = s.search("bending zzzq");
        assert!(o.closest && o.scores[1] > 0);
        assert!(s.search("zzzq qqqz").scores.iter().all(|&x| x == 0));
    }

    /// The Mac's recall set (native/mac/Tests/sheetsearch/queries.tsv): each query's row in the first 3, over every
    /// sheet the native app searches. About a minute in a debug build: `cargo test -p qcalc-win --release
    /// every_query_finds_its_row -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn every_query_finds_its_row() {
        let started = std::time::Instant::now();
        let mut s = Search::default();
        let mut labels = Vec::new();
        let mut copies = Vec::new();
        let mut sheet_of = Vec::new();
        for id in identities::native_sheet_ids() {
            let Some(sheet) = identities::native_identity_sheet_by_id(&id) else {
                continue;
            };
            for section in &sheet.sections {
                for group in &section.groups {
                    for item in &group.items {
                        if item.copy.is_empty() || item.figure.is_some() {
                            continue;
                        }
                        let r = RowText::of(item);
                        s.add(&r, [&sheet.title, &section.title, &group.title]);
                        labels.push(
                            format!(
                                "{} · {} | {} | {}",
                                sheet.title, r.show, r.note, r.explain_title
                            )
                            .to_lowercase(),
                        );
                        copies.push(item.copy.clone());
                        sheet_of.push(id.clone());
                    }
                }
            }
        }
        s.done_adding();
        let index_ms = started.elapsed().as_secs_f64() * 1000.0;
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../mac/Tests/sheetsearch/queries.tsv"
        );
        let text = std::fs::read_to_string(path).expect("queries.tsv");
        let (mut passed, mut first, mut total) = (0, 0, 0);
        let mut times = Vec::new();
        for line in text
            .lines()
            .filter(|l| !l.starts_with('#') && l.contains('\t'))
        {
            let cut: Vec<&str> = line.split('\t').collect();
            let (query, want, open) = (
                cut[0],
                cut[1].to_lowercase(),
                cut.get(2).copied().unwrap_or(""),
            );
            total += 1;
            let t = std::time::Instant::now();
            let o = s.search(query);
            times.push(t.elapsed().as_secs_f64() * 1000.0);
            let mut seen = HashSet::new();
            let ranked: Vec<usize> = Search::ranked(&o, |n| {
                if sheet_of[n] == open {
                    OPEN_SHEET_BONUS
                } else {
                    0
                }
            })
            .into_iter()
            .filter(|&n| seen.insert(copies[n].clone()))
            .collect();
            let hit = ranked
                .iter()
                .take(3)
                .position(|&n| labels[n].contains(&want));
            match hit {
                Some(0) => {
                    passed += 1;
                    first += 1;
                }
                Some(_) => passed += 1,
                None => {
                    println!("MISS {query} -> want {want}");
                    for &n in ranked.iter().take(3) {
                        println!("       {}", labels[n].chars().take(100).collect::<String>());
                    }
                }
            }
        }
        times.sort_by(f64::total_cmp);
        println!(
            "{passed}/{total} in the top 3, {first} first; {} rows indexed in {index_ms:.0} ms; a query's search median {:.2} ms, max {:.2} ms",
            labels.len(),
            times[times.len() / 2],
            times.last().copied().unwrap_or_default()
        );
        assert_eq!(passed, total);
    }
}
