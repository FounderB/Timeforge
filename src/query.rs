//! Parse free-form Ask text into structured hints (path, line, symbol, PR, dig needle).

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ParsedAsk {
    pub raw: String,
    pub pr: Option<String>,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub symbol: Option<String>,
    /// Best token for `git log -S`
    pub dig_needle: Option<String>,
    pub keywords: Vec<String>,
    pub bug_like: bool,
    pub stack_like: bool,
}

/// Extract structure from an Ask string (and optional explicit path).
pub fn parse_ask(query: &str, explicit_path: Option<&str>) -> ParsedAsk {
    let raw = query.trim().to_string();
    let mut out = ParsedAsk {
        raw: raw.clone(),
        path: explicit_path
            .map(|s| s.trim().replace('\\', "/"))
            .filter(|s| !s.is_empty()),
        bug_like: looks_like_bug_ask(&raw),
        stack_like: looks_like_stack(&raw),
        ..ParsedAsk::default()
    };

    if raw.is_empty() {
        return out;
    }

    if let Some(pr) = extract_pr(&raw) {
        out.pr = Some(pr);
        // Bare `#12` / `12` — PR-only ask
        if is_bare_pr(&raw) {
            return out;
        }
    }

    // file:line / path/to/file.rs:42: or Windows-ish
    if let Some((path, line)) = extract_file_line(&raw) {
        if out.path.is_none() {
            out.path = Some(path);
        }
        out.line = line.or(out.line);
    }

    // path-like tokens (src/foo.rs, crates/ignore/src/walk.rs)
    if out.path.is_none() {
        if let Some(p) = extract_path_token(&raw) {
            out.path = Some(p);
        }
    }

    // Rust/C++ style symbol: foo::bar::baz or Class::method
    if let Some(sym) = extract_symbol(&raw) {
        out.symbol = Some(sym.clone());
        out.dig_needle = Some(sym);
    }

    // function-ish: foo( or foo!
    if out.dig_needle.is_none() {
        if let Some(f) = extract_callish(&raw) {
            out.dig_needle = Some(f);
        }
    }

    // keywords / dig needle from remaining words
    out.keywords = meaningful_tokens(&raw);
    if out.dig_needle.is_none() {
        out.dig_needle = pick_dig_needle(&raw, &out.keywords, out.symbol.as_deref());
    }

    // "panic in walk.rs" / "error in src/auth"
    if out.path.is_none() {
        if let Some(p) = extract_in_path(&raw) {
            out.path = Some(p);
        }
    }

    out
}

fn is_bare_pr(q: &str) -> bool {
    let s = q.trim();
    if s.starts_with('#') {
        let rest: String = s.chars().skip(1).collect();
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c.is_whitespace());
    }
    s.chars().all(|c| c.is_ascii_digit()) && s.len() <= 6
}

fn extract_pr(q: &str) -> Option<String> {
    let s = q.trim();
    if let Some(rest) = s.strip_prefix('#') {
        let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !n.is_empty() {
            return Some(n);
        }
    }
    if s.chars().all(|c| c.is_ascii_digit()) && !s.is_empty() && s.len() <= 6 {
        return Some(s.to_string());
    }
    if let Some(rest) = s.to_lowercase().strip_prefix("pull/") {
        let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !n.is_empty() {
            return Some(n);
        }
    }
    // "PR 42" / "pr#42"
    let lower = s.to_lowercase();
    if let Some(idx) = lower.find("pr #").or_else(|| lower.find("pr#")).or_else(|| lower.find("pr "))
    {
        let rest = &s[idx..];
        let digits: String = rest
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !digits.is_empty() {
            return Some(digits);
        }
    }
    None
}

fn extract_file_line(q: &str) -> Option<(String, Option<u32>)> {
    // path/file.ext:123 or path/file.ext:123:45
    for token in q.split_whitespace() {
        let t = token.trim_matches(|c: char| matches!(c, ',' | ')' | '(' | '"' | '\'' | '`'));
        if let Some((path, rest)) = t.rsplit_once(':') {
            if looks_like_path(path) {
                let line = rest
                    .split(':')
                    .next()
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|&n| n > 0);
                if line.is_some() || rest.chars().all(|c| c.is_ascii_digit() || c == ':') {
                    return Some((normalize_path(path), line));
                }
            }
        }
        // path/file.ext alone
        if looks_like_path(t) && t.contains('.') {
            return Some((normalize_path(t), None));
        }
    }
    None
}

fn extract_path_token(q: &str) -> Option<String> {
    for token in q.split_whitespace() {
        let t = token.trim_matches(|c: char| matches!(c, ',' | ')' | '(' | '"' | '\'' | '`'));
        if looks_like_path(t) {
            return Some(normalize_path(t.split(':').next().unwrap_or(t)));
        }
    }
    None
}

fn extract_in_path(q: &str) -> Option<String> {
    let lower = q.to_lowercase();
    for key in [" in ", " at ", " from "] {
        if let Some(idx) = lower.find(key) {
            let rest = q[idx + key.len()..].trim();
            let token = rest
                .split_whitespace()
                .next()?
                .trim_matches(|c: char| matches!(c, ',' | ')' | '(' | '"' | '\'' | '`'));
            let path = token.split(':').next().unwrap_or(token);
            if looks_like_path(path) || path.contains('/') || path.contains('.') {
                return Some(normalize_path(path));
            }
        }
    }
    None
}

fn extract_symbol(q: &str) -> Option<String> {
    for token in q.split_whitespace() {
        let t = token.trim_matches(|c: char| matches!(c, ',' | ')' | '(' | '"' | '\'' | '`' | ':'));
        if t.contains("::") {
            let clean: String = t
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                .collect();
            if clean.contains("::") && clean.len() >= 3 {
                // Prefer last segment for dig if full path is long
                return Some(clean);
            }
        }
    }
    None
}

fn extract_callish(q: &str) -> Option<String> {
    for token in q.split_whitespace() {
        let t = token.trim_matches(|c: char| matches!(c, ',' | ')' | '(' | '"' | '\'' | '`'));
        if let Some(name) = t.strip_suffix("!()").or_else(|| t.strip_suffix('!')) {
            if name.chars().all(|c| c.is_alphanumeric() || c == '_') && name.len() >= 3 {
                return Some(format!("{name}!"));
            }
        }
        if let Some(name) = t.strip_suffix("()") {
            if name.chars().all(|c| c.is_alphanumeric() || c == '_') && name.len() >= 3 {
                return Some(name.to_string());
            }
        }
    }
    None
}

fn looks_like_path(s: &str) -> bool {
    if s.len() < 3 {
        return false;
    }
    if s.contains("://") {
        return false;
    }
    let lower = s.to_lowercase();
    let exts = [
        ".rs", ".go", ".py", ".js", ".ts", ".tsx", ".jsx", ".c", ".h", ".cpp", ".hpp", ".java",
        ".kt", ".swift", ".rb", ".php", ".cs", ".md", ".toml", ".yml", ".yaml", ".json", ".sh",
    ];
    if exts.iter().any(|e| lower.ends_with(e)) {
        return true;
    }
    s.contains('/')
        && !s.starts_with("http")
        && s.chars().all(|c| {
            c.is_alphanumeric() || matches!(c, '/' | '_' | '-' | '.' | '\\')
        })
}

fn normalize_path(s: &str) -> String {
    s.trim().trim_start_matches("./").replace('\\', "/")
}

fn meaningful_tokens(q: &str) -> Vec<String> {
    q.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|t| t.len() >= 3 && !is_stopword(t))
        .map(|t| t.to_string())
        .collect()
}

fn pick_dig_needle(raw: &str, keywords: &[String], symbol: Option<&str>) -> Option<String> {
    if let Some(s) = symbol {
        return Some(s.to_string());
    }
    if !raw.contains(' ') && !is_stopword(raw) && raw.len() >= 2 {
        return Some(raw.trim().to_string());
    }
    let mut best: Option<&str> = None;
    for t in keywords {
        best = Some(match best {
            None => t.as_str(),
            Some(b) if looks_like_code_token(t) && !looks_like_code_token(b) => t.as_str(),
            Some(b) if t.len() > b.len() => t.as_str(),
            Some(b) => b,
        });
    }
    best.map(|s| s.to_string())
}

pub fn looks_like_code_token(q: &str) -> bool {
    let s = q.trim();
    if s.len() < 2 {
        return false;
    }
    if s.contains("::") || s.contains("->") || s.contains('_') || s.contains('!') {
        return true;
    }
    let has_lower = s.chars().any(|c| c.is_lowercase());
    let has_upper = s.chars().any(|c| c.is_uppercase());
    if has_lower && has_upper && !s.contains(' ') {
        return true;
    }
    !s.contains(' ')
        && s.chars().filter(|c| c.is_alphanumeric()).count() >= s.len().saturating_mul(3) / 4
        && s.len() >= 4
}

pub fn looks_like_bug_ask(q: &str) -> bool {
    let s = q.to_lowercase();
    [
        "bug", "fix", "panic", "crash", "regress", "broken", "fail", "error", "unwrap",
        "segfault", "null", "oom", "timeout", "flake", "deadlock", "overflow", "exception",
        "traceback", "stack",
    ]
    .iter()
    .any(|w| s.contains(w))
}

fn looks_like_stack(q: &str) -> bool {
    let s = q.to_lowercase();
    s.contains("traceback")
        || s.contains("stack backtrace")
        || s.contains(" at ") && (s.contains(".rs:") || s.contains(".go:") || s.contains(".py:"))
        || s.lines().count() > 1 && (s.contains("::") || s.contains(".rs:"))
}

pub fn is_stopword(q: &str) -> bool {
    matches!(
        q.trim().to_lowercase().as_str(),
        "fix"
            | "bug"
            | "bugs"
            | "error"
            | "errors"
            | "test"
            | "tests"
            | "add"
            | "update"
            | "merge"
            | "release"
            | "initial"
            | "docs"
            | "doc"
            | "typo"
            | "chore"
            | "ci"
            | "feat"
            | "feature"
            | "patch"
            | "hotfix"
            | "issue"
            | "pr"
            | "and"
            | "the"
            | "for"
            | "with"
            | "from"
            | "this"
            | "that"
            | "when"
            | "what"
            | "why"
            | "into"
            | "over"
            | "under"
            | "after"
            | "before"
            | "line"
            | "file"
            | "src"
            | "lib"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_file_line() {
        let p = parse_ask("panic at src/auth.rs:42", None);
        assert_eq!(p.path.as_deref(), Some("src/auth.rs"));
        assert_eq!(p.line, Some(42));
        assert!(p.bug_like);
    }

    #[test]
    fn parses_symbol() {
        let p = parse_ask("error in foo::bar::baz", None);
        assert_eq!(p.symbol.as_deref(), Some("foo::bar::baz"));
        assert_eq!(p.dig_needle.as_deref(), Some("foo::bar::baz"));
    }

    #[test]
    fn parses_pr() {
        let p = parse_ask("#42", None);
        assert_eq!(p.pr.as_deref(), Some("42"));
        assert!(p.dig_needle.is_none());
    }

    #[test]
    fn parses_in_path() {
        let p = parse_ask("deadlock in crates/ignore/src/walk.rs", None);
        assert_eq!(p.path.as_deref(), Some("crates/ignore/src/walk.rs"));
        assert!(p.dig_needle.as_deref() == Some("deadlock") || p.bug_like);
    }

    #[test]
    fn dig_skips_stopwords() {
        let p = parse_ask("fix payment", None);
        assert_eq!(p.dig_needle.as_deref(), Some("payment"));
    }
}
