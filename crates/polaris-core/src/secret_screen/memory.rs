//! The privacy screen for the turn-only memory, with rules of its own.
//!
//! [`screen_text`](super::screen_text) guards the audit log, MCP and the final
//! answer, and leans toward redacting too much: it takes about a quarter of the
//! identifiers of 20 characters or more in this repository's own sources for
//! secrets. A memory that quotes what it kept cannot afford that, so this screen
//! shares none of those rules. Each rule names a way a secret shows itself; none
//! looks at the mere shape of a word.
//!
//! - A token that gives itself away by its prefix (`sk-`, `ghp_`, `AKIA`...),
//!   whatever characters follow.
//! - The value of a secret-named key in YAML or JSON, of an assignment
//!   (`API_KEY=...`) or of a long flag (`--password ...`). `max_tokens` counts
//!   something; `GITHUB_TOKEN` holds something.
//! - An `Authorization` header, a `Bearer` token, a password in a URL.
//! - A long random string and nothing else: never one with `_`, `/` or `-`
//!   (identifiers, paths), and never one made of words (`RewindBoundaryCompletionV1`).
//!
//! A value that only refers to a secret (`${DB_PASSWORD}`, `<password>`) or that
//! is code (a type, a call, a field ending its line) is left alone. Redacting
//! text that was redacted leaves it as it was.

use std::sync::LazyLock;

use regex::{Captures, Regex};

use super::{FilterResult, PLACEHOLDER, nothing_useful_remains};

/// A string this long, and random, is a secret even with nothing around it.
const RANDOM_MIN_LEN: usize = 24;
/// A mixed-case string is an identifier if at least this share of it is words.
const IDENTIFIER_WORD_SHARE: f64 = 0.7;

/// Words that mark a name as holding a secret wherever they stand in it.
const SECRET_WORDS: &[&str] = &[
    "secret",
    "secrets",
    "password",
    "passwords",
    "passwd",
    "passphrase",
    "pwd",
    "credential",
    "credentials",
    "apikey",
];
/// `key` holds a secret only after one of these (`sort_key` does not).
const KEY_OWNERS: &[&str] = &[
    "api",
    "private",
    "access",
    "signing",
    "encryption",
    "ssh",
    "master",
    "license",
    "auth",
    "hmac",
    "deploy",
];

fn rule(pattern: &str) -> Regex {
    Regex::new(pattern).expect("rule compiles")
}

/// A token whose prefix gives it away. The characters after it do not matter.
static PREFIXED: LazyLock<Regex> = LazyLock::new(|| {
    rule(
        &[
            r"\bsk-[A-Za-z0-9_-]{8,}",
            r"\bgh[pousr]_[A-Za-z0-9]{8,}",
            r"\bgithub_pat_[A-Za-z0-9_]{8,}",
            r"\bxox[abprs]-[A-Za-z0-9-]{8,}",
            r"\b(?:AKIA|ASIA|AGPA|AIDA|AROA|AIPA|ANPA|ANVA)[A-Z0-9]{12,}",
            r"\bAIza[A-Za-z0-9_-]{20,}",
            r"\bglpat-[A-Za-z0-9_-]{10,}",
            r"\b[sr]k_(?:live|test)_[A-Za-z0-9]{8,}",
            r"\bnpm_[A-Za-z0-9]{20,}",
            r"\bpypi-[A-Za-z0-9_-]{20,}",
            r"\bhf_[A-Za-z0-9]{20,}",
            r"\bya29\.[A-Za-z0-9_-]{20,}",
            r"\bSG\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}",
            r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{4,}",
        ]
        .join("|"),
    )
});

/// A PEM private key, up to its end line or the end of the text.
static PRIVATE_KEY: LazyLock<Regex> = LazyLock::new(|| {
    rule(r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?(?:-----END [A-Z ]*PRIVATE KEY-----|\z)")
});

/// `key: value` in YAML, and `"key": "value"` in JSON. Capture 1 is the key,
/// 2 the colon with what surrounds it, 3 the value (quoted, or bare up to
/// whatever ends it).
static KEY_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    rule(
        r#"([A-Za-z0-9_.-]+)(["'`]?[ \t]*:[ \t]*)("[^"\n]*"|'[^'\n]*'|[^\s,;{}()\[\]<>"'`:][^\s,;{}()\[\]<>"'`]*)"#,
    )
});

/// `NAME=value`, as an environment variable or a query parameter, or
/// `NAME = "value"` as a setting. Capture 1 is the name, 2 the equals sign, 3
/// the value.
static ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    rule(r#"\b([A-Za-z_][A-Za-z0-9_.-]*)([ \t]*=[ \t]*)("[^"\n]*"|'[^'\n]*'|\S+)"#)
});

/// `--flag value`; the `--flag=value` form is an assignment.
static LONG_FLAG: LazyLock<Regex> =
    LazyLock::new(|| rule(r#"--([A-Za-z][A-Za-z0-9_-]*)([ \t]+)("[^"\n]*"|'[^'\n]*'|[^\s-]\S*)"#));

/// An `Authorization` header that names its scheme.
static AUTH_SCHEME: LazyLock<Regex> = LazyLock::new(|| {
    rule(r#"(?i)\b(authorization\s*[:=]\s*(?:bearer|basic|token|digest|negotiate)\s+)([^\s'",;]+)"#)
});

/// An `Authorization` header that does not.
static AUTH_VALUE: LazyLock<Regex> =
    LazyLock::new(|| rule(r"(?i)\b(authorization\s*[:=]\s*)([A-Za-z0-9._~+/=-]{8,})"));

/// A `Bearer` token outside a header.
static BEARER: LazyLock<Regex> =
    LazyLock::new(|| rule(r"(?i)\b(bearer\s+)([A-Za-z0-9._~+/=-]{8,})"));

/// `scheme://user:password@host`. Capture 1 is the user, 2 the password.
static URL_PASSWORD: LazyLock<Regex> = LazyLock::new(|| rule(r"://([^:/\s@]+):([^@\s/]+)@"));

/// `scheme://token@host`; a short user name (`git`) is not one.
static URL_TOKEN: LazyLock<Regex> = LazyLock::new(|| rule(r"://([^:@/\s]+)@"));

/// A run that may be a random string; `looks_random` decides.
static RUN: LazyLock<Regex> = LazyLock::new(|| rule(r"[A-Za-z0-9+/_=-]{24,}"));

/// Screens `s` for the turn-only memory: [`FilterResult::Keep`] if nothing was
/// found, [`FilterResult::Redacted`] with the secrets replaced, or
/// [`FilterResult::Drop`] if nothing but the secrets was there.
pub fn screen_for_memory(s: &str) -> FilterResult {
    let mut text = s.to_owned();
    let mut found = false;
    let mut step = |rule: &Regex, decide: &dyn Fn(&Captures, &str) -> Option<String>| {
        let mut hit = false;
        let out = rule.replace_all(&text, |caps: &Captures| {
            let whole = caps.get(0).expect("whole match");
            match decide(caps, &text[whole.end()..]) {
                Some(replacement) => {
                    hit = true;
                    replacement
                }
                None => whole.as_str().to_owned(),
            }
        });
        if hit {
            text = out.into_owned();
            found = true;
        }
    };

    step(&PRIVATE_KEY, &|_, _| Some(PLACEHOLDER.into()));
    step(&PREFIXED, &|_, _| Some(PLACEHOLDER.into()));
    step(&KEY_VALUE, &|caps, rest| {
        let value = &caps[3];
        let (quote, inner) = unquote(value);
        let code = quote.is_empty() && (value.contains("::") || follows_code(rest));
        (secret_named(&caps[1]) && !is_reference(inner) && !code)
            .then(|| format!("{}{}{}", &caps[1], &caps[2], mask(value)))
    });
    step(&ASSIGNMENT, &|caps, _| {
        let value = &caps[3];
        let (quote, inner) = unquote(value);
        // Without quotes only `NAME=value` is an assignment; `a = b` is code.
        let spaced = caps[2].contains([' ', '\t']);
        (secret_named(&caps[1]) && !is_reference(inner) && !(spaced && quote.is_empty()))
            .then(|| format!("{}{}{}", &caps[1], &caps[2], mask(value)))
    });
    step(&LONG_FLAG, &|caps, _| {
        let value = &caps[3];
        (secret_named(&caps[1]) && !is_reference(unquote(value).1))
            .then(|| format!("--{}{}{}", &caps[1], &caps[2], mask(value)))
    });
    step(&AUTH_SCHEME, &|caps, _| {
        (!is_reference(&caps[2])).then(|| format!("{}{PLACEHOLDER}", &caps[1]))
    });
    step(&AUTH_VALUE, &|caps, rest| {
        let value = &caps[2];
        (value.bytes().any(|b| b.is_ascii_digit()) && !is_reference(value) && !follows_code(rest))
            .then(|| format!("{}{PLACEHOLDER}", &caps[1]))
    });
    step(&BEARER, &|caps, rest| {
        let value = &caps[2];
        let credential = value.len() >= 20 || value.bytes().any(|b| b.is_ascii_digit());
        (credential && !is_reference(value) && !follows_code(rest))
            .then(|| format!("{}{PLACEHOLDER}", &caps[1]))
    });
    step(&URL_PASSWORD, &|caps, _| {
        (&caps[2] != PLACEHOLDER).then(|| format!("://{}:{PLACEHOLDER}@", &caps[1]))
    });
    step(&URL_TOKEN, &|caps, _| {
        (caps[1].chars().count() >= 16 && !is_reference(&caps[1]))
            .then(|| format!("://{PLACEHOLDER}@"))
    });
    step(&RUN, &|caps, _| {
        looks_random(&caps[0]).then(|| PLACEHOLDER.into())
    });

    if !found {
        return FilterResult::Keep(s.to_owned());
    }
    if nothing_useful_remains(&text) {
        return FilterResult::Drop;
    }
    FilterResult::Redacted(text)
}

/// The words of a name, lowercase, split at anything but a letter or digit and
/// at case changes: `GITHUB_TOKEN` and `apiKey` are two words each.
fn words(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut out = Vec::new();
    let mut word = String::new();
    for (at, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
            continue;
        }
        // A capital opens a word after a lowercase letter or a digit, and
        // after capitals when a lowercase letter follows (`APIKey`).
        let opens = c.is_uppercase()
            && !word.is_empty()
            && (chars[at - 1].is_lowercase()
                || chars[at - 1].is_ascii_digit()
                || (chars[at - 1].is_uppercase()
                    && chars.get(at + 1).is_some_and(|next| next.is_lowercase())));
        if opens {
            out.push(std::mem::take(&mut word));
        }
        word.extend(c.to_lowercase());
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// Whether a name is one a secret is kept under. A word that counts
/// (`max_tokens`) or describes (`token_file`) does not make one.
fn secret_named(name: &str) -> bool {
    let words = words(name);
    let last = words.last().map(String::as_str);
    let before_last = words
        .len()
        .checked_sub(2)
        .map(|at| words[at].as_str())
        .unwrap_or("");
    words.iter().any(|w| SECRET_WORDS.contains(&w.as_str()))
        || last == Some("token")
        || (!before_last.is_empty() && last == Some("pass"))
        || (last == Some("key") && KEY_OWNERS.contains(&before_last))
}

/// A value that only points at one: empty, a variable, a template, a
/// placeholder, a YAML construct that is not a plain scalar, or a word for
/// nothing. A secret does not start with these.
fn is_reference(value: &str) -> bool {
    let value = value.trim();
    let variable = value.strip_prefix('$').is_some_and(|name| {
        name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    });
    value.is_empty()
        || value.starts_with(['<', '[', '{', '(', '%', '&', '*', '!', '|', '>', '@', '`'])
        || value.starts_with("${")
        || variable
        || ["null", "true", "false", "none", "nil", "undefined", "~"]
            .iter()
            .any(|word| value.eq_ignore_ascii_case(word))
}

/// Whether what follows a bare value shows it to be code and not a value: a
/// call, a path, generics, a struct literal, a binding, or a field that ends
/// its line.
fn follows_code(rest: &str) -> bool {
    let line = rest.split('\n').next().unwrap_or("");
    let line = line.trim_start_matches([' ', '\t']);
    line.starts_with(['(', ')', '<', '{', '}', '[', ']', '='])
        || line.starts_with("::")
        || matches!(line.trim_end(), "," | ";")
}

/// A value split into the quote around it (empty if bare) and what is inside.
fn unquote(value: &str) -> (&str, &str) {
    match value.chars().next() {
        Some(quote @ ('"' | '\'')) if value.len() >= 2 && value.ends_with(quote) => {
            (&value[..1], &value[1..value.len() - 1])
        }
        _ => ("", value),
    }
}

/// The placeholder where `value` was, inside the quotes it had.
fn mask(value: &str) -> String {
    let (quote, _) = unquote(value);
    format!("{quote}{PLACEHOLDER}{quote}")
}

/// Whether a run of `[A-Za-z0-9+/_=-]` is a long random string. A run with
/// `_`, `/` or `-` is an identifier, a path or a UUID, and one with `=` inside
/// is a setting. The rest must hold a digit and a letter, not be a git hash,
/// and look random: hex by its spread, mixed case by its spread and by not
/// being made of words.
fn looks_random(run: &str) -> bool {
    if run.contains(['_', '/', '-']) {
        return false;
    }
    let token = run.trim_end_matches('=');
    if token.len() < RANDOM_MIN_LEN || token.contains('=') {
        return false;
    }
    let any = |test: fn(&u8) -> bool| token.bytes().any(|b| test(&b));
    let (lower, upper, digit) = (
        any(u8::is_ascii_lowercase),
        any(u8::is_ascii_uppercase),
        any(u8::is_ascii_digit),
    );
    if !digit || !(lower || upper) {
        return false;
    }
    if token
        .bytes()
        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        && matches!(token.len(), 40 | 64)
    {
        return false;
    }
    let spread = entropy(token);
    if token.bytes().all(|b| b.is_ascii_hexdigit()) {
        spread >= 3.0
    } else if lower && upper {
        spread >= 3.5 && word_share(token) < IDENTIFIER_WORD_SHARE
    } else {
        spread >= 3.5
    }
}

/// Shannon entropy of the bytes, in bits per byte.
fn entropy(token: &str) -> f64 {
    let mut counts = [0usize; 256];
    for b in token.bytes() {
        counts[usize::from(b)] += 1;
    }
    let len = token.len() as f64;
    counts
        .iter()
        .filter(|&&n| n > 0)
        .map(|&n| {
            let p = n as f64 / len;
            -p * p.log2()
        })
        .sum()
}

/// The share of an ASCII `token` that lies in words: three or more letters of
/// one case, a capital leading at most (`Rewind`, `Boundary`, `CONFIG`). An
/// identifier is made of them and a random string is not.
fn word_share(token: &str) -> f64 {
    let bytes = token.as_bytes();
    let run =
        |from: usize, test: fn(&u8) -> bool| bytes[from..].iter().take_while(|b| test(b)).count();
    let (mut at, mut covered) = (0, 0);
    while at < bytes.len() {
        let b = bytes[at];
        let length = if b.is_ascii_uppercase() {
            let capitals = run(at, u8::is_ascii_uppercase);
            if bytes.get(at + capitals).is_some_and(u8::is_ascii_lowercase) {
                // The last capital opens a word; the ones before it are an acronym.
                let acronym = capitals - 1;
                if acronym >= 3 {
                    covered += acronym;
                }
                at += acronym;
                1 + run(at + 1, u8::is_ascii_lowercase)
            } else {
                capitals
            }
        } else if b.is_ascii_lowercase() {
            run(at, u8::is_ascii_lowercase)
        } else {
            // Digits and the rest are never words.
            at += run(at, |b| !b.is_ascii_alphabetic()).max(1);
            continue;
        };
        if length >= 3 {
            covered += length;
        }
        at += length;
    }
    covered as f64 / bytes.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the screen left of `s`.
    fn screened(s: &str) -> String {
        match screen_for_memory(s) {
            FilterResult::Keep(text) | FilterResult::Redacted(text) => text,
            FilterResult::Drop => String::new(),
        }
    }

    /// Secrets in the forms the rules name, each with the secret in it.
    const SECRETS: &[(&str, &str)] = &[
        ("password: hunter2hunter2", "hunter2hunter2"),
        ("  api_key: \"abcd1234efgh5678\"", "abcd1234efgh5678"),
        (
            "{\"access_token\": \"tok_ABCdef0123456789\", \"user\": \"bob\"}",
            "tok_ABCdef0123456789",
        ),
        ("- DB_PASSWORD=hunter2hunter2", "hunter2hunter2"),
        ("API_KEY = \"abcd1234efgh\"", "abcd1234efgh"),
        ("x-api-key=abc123xyz", "abc123xyz"),
        ("MYSQL_PWD=hunter2 mysql", "hunter2"),
        ("mysql --password hunter2hunter2 -e x", "hunter2hunter2"),
        ("tool --api-key=abcdef12345", "abcdef12345"),
        (
            "export OPENAI_API_KEY=sk-proj-abcdefghijklmnop",
            "abcdefghijklmnop",
        ),
        ("sk-alpha-bravo-charlie-delta-echo", "charlie-delta"),
        (
            "ghp_abcdefghijklmnopqrstuvwxyz0123456789",
            "abcdefghijklmnop",
        ),
        ("see github_pat_11ABCDEFG0abcdefghijkl_mnop", "abcdefghijkl"),
        ("slack xoxb-123456789012-abcdefghijkl", "abcdefghijkl"),
        ("key AKIAIOSFODNN7EXAMPLE here", "AKIAIOSFODNN7EXAMPLE"),
        (
            "Authorization: Bearer abc123def456ghi789",
            "abc123def456ghi789",
        ),
        (
            "curl -H 'Authorization: Basic dXNlcjpwYXNzd29yZA=='",
            "dXNlcjpwYXNzd29yZA",
        ),
        ("Authorization: abc123def456ghi789", "abc123def456ghi789"),
        ("send Bearer abc123def456ghi789 now", "abc123def456ghi789"),
        ("postgres://admin:s3cr3tPass@db:5432/app", "s3cr3tPass"),
        (
            "https://ghp_abcdefghijklmnopqrstuvwxyz0123456789@github.com/o/r",
            "abcdefghijklmnop",
        ),
        (
            "https://abcdefghijklmnopqrstu@host/x",
            "abcdefghijklmnopqrstu",
        ),
        (
            "my key is Zq8Lm2Xp4Rt6Vb1Nc3Hd5Jf7Gs9Kw0Aa ok",
            "Zq8Lm2Xp4Rt6Vb1Nc3Hd5Jf7Gs9Kw0Aa",
        ),
        (
            "a 0123456789abcdef0123456789abcdef b",
            "0123456789abcdef0123456789abcdef",
        ),
        (
            "t eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N",
            "dozjgNryP4J3jVmNHl0w5N",
        ),
        (
            "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEF\n-----END PRIVATE KEY-----",
            "MIIEvQIBADANBgkqhkiG9w0BAQEF",
        ),
        (
            "Please set `api_key: abcd1234efgh` in config",
            "abcd1234efgh",
        ),
        ("the password: correcthorse", "correcthorse"),
    ];

    #[test]
    fn a_secret_in_any_form_the_rules_name_is_redacted_and_what_surrounds_it_stays() {
        for (text, secret) in SECRETS {
            let out = screened(text);
            assert!(!out.contains(secret), "{text:?} left {out:?}");
            assert!(
                matches!(
                    screen_for_memory(text),
                    FilterResult::Redacted(_) | FilterResult::Drop
                ),
                "{text:?}"
            );
        }
        // The context is kept: the key, the host, the flag, the words.
        assert_eq!(screened("password: hunter2hunter2"), "password: [REDACTED]");
        assert_eq!(
            screened("postgres://admin:s3cr3tPass@db:5432/app"),
            "postgres://admin:[REDACTED]@db:5432/app"
        );
        assert_eq!(
            screened("{\"access_token\": \"tok_ABC123\", \"user\": \"bob\"}"),
            "{\"access_token\": \"[REDACTED]\", \"user\": \"bob\"}"
        );
        assert_eq!(
            screened("Please set `api_key: abcd1234efgh` in config"),
            "Please set `api_key: [REDACTED]` in config"
        );
        assert_eq!(
            screened("mysql --password hunter2hunter2 -e x"),
            "mysql --password [REDACTED] -e x"
        );
        // A text of nothing but a secret has nothing left worth keeping.
        assert_eq!(
            screen_for_memory("ghp_abcdefghijklmnopqrstuvwxyz0123456789"),
            FilterResult::Drop
        );
    }

    #[test]
    fn redacting_what_was_redacted_changes_nothing() {
        for (text, _) in SECRETS {
            let once = screened(text);
            let twice = screened(&once);
            assert_eq!(once, twice, "{text:?}");
        }
    }

    /// What the memory has to be able to quote as it is. The first three are
    /// what `screen_text` redacts and this screen must not.
    const KEPT: &[&str] = &[
        "RewindBoundaryCompletionV1",
        "Sources/PolarisSettings/BootstrapV2Publisher",
        "max_tokens=256",
        "git show 8f3a1c2e9b7d4560112233445566778899aabbc0",
        "550e8400-e29b-41d4-a716-446655440000",
        "foo_bar_baz_qux_quux_corge_grault_garply_waldo_fred_plugh",
        "WorkflowArtifactBodiesSnapshotV1 and D27ManifestProjectionEntryV1",
        "crates/polaris-core/src/turn_memory.rs",
        "tokens_per_minute=60000 token_count: 5",
        "pub token: Option<String>,",
        "token: JoinedManagementEffectRecoveryV1,",
        "fn f(token: String) -> Self {",
        "let secret: &str = \"abc\";",
        "let token: u32 = 5;",
        "token: String::new(),",
        "credential: BaselineDocumentRecordV2::credential(",
        "pub authorization: CredentialMutationAuthorizationV1,",
        "authorization: HeaderValue,",
        "password: ${DB_PASSWORD}",
        "password: <your-password>",
        "token: \"\"",
        "TOKEN=$OTHER",
        "GITHUB_TOKEN=${{ secrets.GITHUB_TOKEN }}",
        "the bearer token is required",
        "docker run -u 1000:1000 img",
        "ssh://git@github.com/user/repo",
        "http://localhost:8080/foo@bar",
        "sk-learn is a library",
        "token = get_token()",
    ];

    #[test]
    fn what_a_memory_has_to_quote_is_kept_as_it_is() {
        for text in KEPT {
            assert_eq!(
                screen_for_memory(text),
                FilterResult::Keep((*text).to_owned()),
                "{text:?}"
            );
        }
        // The premise: the shared screen redacts the first three.
        for text in &KEPT[..3] {
            assert!(
                !matches!(super::super::screen_text(text), FilterResult::Keep(_)),
                "screen_text kept {text:?}"
            );
        }
    }

    #[test]
    fn a_name_holds_a_secret_by_its_words_not_its_letters() {
        for name in [
            "GITHUB_TOKEN",
            "apiKey",
            "APIKey",
            "x-api-key",
            "AWS_SECRET_ACCESS_KEY",
            "DB_PASS",
            "private_key",
            "client_secret",
            "spring.datasource.password",
        ] {
            assert!(secret_named(name), "{name}");
        }
        for name in [
            "max_tokens",
            "token_count",
            "TOKENIZER",
            "sort_key",
            "primary_key",
            "pass",
            "bypass_cache",
            "AWS_ACCESS_KEY_ID",
            "monkey",
        ] {
            assert!(!secret_named(name), "{name}");
        }
    }

    #[test]
    fn a_random_string_is_told_from_an_identifier_by_its_spread_and_its_words() {
        for random in [
            "Zq8Lm2Xp4Rt6Vb1Nc3Hd5Jf7Gs9Kw0Aa",
            "0123456789abcdef0123456789abcdef",
            "k3j9x0v8b7n6m5q4w3e2r1t0y9u8i7o6p",
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw",
        ] {
            assert!(looks_random(random), "{random}");
        }
        for not_random in [
            // Words.
            "RewindBoundaryCompletionV1",
            "WorkflowArtifactBodiesSnapshotV1",
            "C10CompletedParticipationRecoveryV1",
            "kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly",
            // A git hash, a UUID, a path, an identifier, a setting, a short string.
            "8f3a1c2e9b7d4560112233445566778899aabbc0",
            "550e8400-e29b-41d4-a716-446655440000",
            "Sources/PolarisSettings/BootstrapV2Publisher",
            "snake_case_identifier_that_runs_long_0123",
            "repositoryformatversion=0000000000",
            "Zq8Lm2Xp4Rt6Vb1Nc3Hd5J",
        ] {
            assert!(!looks_random(not_random), "{not_random}");
        }
    }
}
