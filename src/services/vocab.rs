use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::domain::ScreenContext;

/// Maximum number of vocabulary terms to ship with a request. Beyond this we
/// just spend prompt budget for diminishing returns.
const MAX_TERMS: usize = 32;

/// Extract candidate vocabulary terms from on-screen text. We bias toward
/// rare/proper-noun-like tokens that Whisper would otherwise mangle.
///
/// Heuristics:
/// - Capitalized tokens that aren't sentence-initial English words
/// - CamelCase / mixedCase tokens
/// - `@handles`
/// - Tokens containing non-ASCII characters (likely names in non-English
///   scripts — Malayalam, Devanagari, etc.)
/// - Tokens with mixed letters and digits (e.g. version strings, model names)
///
/// We deduplicate case-insensitively and order by frequency, then truncate.
pub fn extract_terms(context: &ScreenContext) -> Vec<String> {
    let mut counts: HashMap<String, (usize, String)> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    let mut sources: Vec<&str> = vec![
        context.window_title.as_str(),
        context.focused_value_preview.as_str(),
        context.visible_text.as_str(),
    ];
    sources.retain(|s| !s.is_empty());

    for source in sources {
        for raw in source.split(|c: char| {
            c.is_whitespace()
                || c == ','
                || c == '.'
                || c == '!'
                || c == '?'
                || c == ';'
                || c == ':'
                || c == '('
                || c == ')'
                || c == '['
                || c == ']'
                || c == '"'
        }) {
            let token = raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '@' && c != '_');
            if !is_interesting(token) {
                continue;
            }
            let key = token.to_lowercase();
            counts
                .entry(key.clone())
                .and_modify(|(count, _)| *count += 1)
                .or_insert_with(|| {
                    order.push(key.clone());
                    (1, token.to_string())
                });
        }
    }

    // Sort by descending frequency, then by first-seen order for stability.
    let mut indexed: Vec<(usize, String)> = order
        .into_iter()
        .map(|key| {
            let (count, original) = counts.remove(&key).unwrap();
            (count, original)
        })
        .collect();
    indexed.sort_by(|a, b| b.0.cmp(&a.0));

    indexed
        .into_iter()
        .map(|(_, term)| term)
        .take(MAX_TERMS)
        .collect()
}

fn is_interesting(token: &str) -> bool {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() < 2 {
        return false;
    }

    let has_non_ascii = chars.iter().any(|c| !c.is_ascii());
    if has_non_ascii && chars.iter().any(|c| c.is_alphabetic()) {
        return true;
    }

    if token.starts_with('@') && chars.len() >= 3 {
        return true;
    }

    let has_letter = chars.iter().any(|c| c.is_alphabetic());
    let has_digit = chars.iter().any(|c| c.is_ascii_digit());
    if has_letter && has_digit {
        return true;
    }

    let upper_count = chars.iter().filter(|c| c.is_uppercase()).count();
    let lower_count = chars.iter().filter(|c| c.is_lowercase()).count();

    // CamelCase / mixedCase: at least one upper after the first char, plus
    // at least one lower.
    if upper_count >= 2 && lower_count >= 1 {
        return true;
    }

    // Capitalized token at least 4 chars long. We exclude very common short
    // words to keep the prompt budget tight.
    if chars[0].is_uppercase() && lower_count >= 3 && !is_common_word(token) {
        return true;
    }

    false
}

fn is_common_word(token: &str) -> bool {
    matches!(
        token.to_ascii_lowercase().as_str(),
        "the"
            | "this"
            | "that"
            | "these"
            | "those"
            | "with"
            | "from"
            | "your"
            | "there"
            | "their"
            | "they"
            | "them"
            | "what"
            | "when"
            | "where"
            | "which"
            | "while"
            | "would"
            | "could"
            | "should"
            | "have"
            | "here"
            | "about"
            | "into"
            | "over"
            | "under"
            | "after"
            | "before"
            | "again"
    )
}

/// Persistent per-user vocabulary store. Frequencies accumulate across
/// sessions so frequently-seen names ("Johnykutty", project codenames, etc.)
/// always get prepended to the Whisper prompt regardless of what's currently
/// on screen.
pub struct PersonalVocab {
    path: PathBuf,
    counts: Mutex<HashMap<String, (usize, String)>>,
}

impl PersonalVocab {
    pub fn load() -> Self {
        let path = default_path();
        let counts = match path.as_ref().and_then(|p| fs::read_to_string(p).ok()) {
            Some(text) => parse_store(&text),
            None => HashMap::new(),
        };

        Self {
            path: path.unwrap_or_else(|| PathBuf::from("/tmp/rust-assistant-vocab.txt")),
            counts: Mutex::new(counts),
        }
    }

    /// Record that these terms were observed. Increments their counts and
    /// persists to disk best-effort (failures are ignored — vocab is a
    /// nice-to-have, not load-bearing).
    pub fn record(&self, terms: &[String]) {
        if terms.is_empty() {
            return;
        }
        if let Ok(mut counts) = self.counts.lock() {
            for term in terms {
                let key = term.to_lowercase();
                counts
                    .entry(key)
                    .and_modify(|(c, _)| *c += 1)
                    .or_insert_with(|| (1, term.clone()));
            }
            let _ = persist(&self.path, &counts);
        }
    }

    /// Return up to `limit` terms ordered by descending frequency.
    pub fn top(&self, limit: usize) -> Vec<String> {
        let Ok(counts) = self.counts.lock() else {
            return Vec::new();
        };
        let mut entries: Vec<(usize, String)> =
            counts.values().map(|(c, t)| (*c, t.clone())).collect();
        entries.sort_by(|a, b| b.0.cmp(&a.0));
        entries.into_iter().map(|(_, t)| t).take(limit).collect()
    }
}

fn default_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let mut path = PathBuf::from(home);
    path.push(".config");
    path.push("rust-assistant");
    let _ = fs::create_dir_all(&path);
    path.push("vocab.txt");
    Some(path)
}

fn parse_store(text: &str) -> HashMap<String, (usize, String)> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let Some((count_str, term)) = line.split_once('\t') else {
            continue;
        };
        let Ok(count) = count_str.parse::<usize>() else {
            continue;
        };
        let term = term.trim();
        if term.is_empty() {
            continue;
        }
        map.insert(term.to_lowercase(), (count, term.to_string()));
    }
    map
}

fn persist(path: &PathBuf, counts: &HashMap<String, (usize, String)>) -> std::io::Result<()> {
    let mut entries: Vec<(usize, &str)> =
        counts.values().map(|(c, t)| (*c, t.as_str())).collect();
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    // Cap on disk to avoid unbounded growth.
    entries.truncate(2_000);

    let mut out = String::new();
    for (count, term) in entries {
        out.push_str(&format!("{count}\t{term}\n"));
    }
    fs::write(path, out)
}

/// Combine on-screen extracted terms with the user's persistent vocabulary,
/// keeping screen terms first (most contextually relevant) and falling back
/// to persistent terms to fill the budget.
pub fn merged_terms(context: &ScreenContext, personal: &PersonalVocab, limit: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for term in extract_terms(context) {
        let key = term.to_lowercase();
        if seen.insert(key) {
            out.push(term);
        }
        if out.len() >= limit {
            return out;
        }
    }

    for term in personal.top(limit) {
        let key = term.to_lowercase();
        if seen.insert(key) {
            out.push(term);
        }
        if out.len() >= limit {
            break;
        }
    }

    out
}
