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
//!   something; `GITHUB_TOKEN` holds something. A bare value with words after
//!   it takes them to the end of the line (`password: correct horse battery
//!   staple`). A key and its value are read only from a secret-named key on, so
//!   a pair before it (`INFO: password: ...`) does not hide it.
//! - An `Authorization` header, a `Bearer` token, a password in a URL.
//! - A long random string and nothing else. One with `_`, `/` or `-` in it is
//!   judged by its pieces between them: an identifier, a path, a UUID or a
//!   version is made of words, numbers and hashes and stays
//!   (`Sources/PolarisSettings/BootstrapV2Publisher`), a base64 key is not. A
//!   string made of words (`RewindBoundaryCompletionV1`) stays too.
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
/// A run with `_`, `/` or `-` is random only if the pieces between them that
/// are not words, numbers or hashes add up to at least this many characters.
const RANDOM_LOOSE_MIN_LEN: usize = 16;
/// How much of a line after a value is read to tell words from code.
const LINE_START_LEN: usize = 200;

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

/// The key of `key: value` in YAML and of `"key": "value"` in JSON, and the
/// colon with what surrounds it. Capture 1 is the key, 2 the colon.
/// `KEY_VALUE` reads the value after it.
static KEY: LazyLock<Regex> = LazyLock::new(|| rule(r#"([A-Za-z0-9_.-]+)(["'`]?[ \t]*:[ \t]*)"#));

/// A value after a colon: quoted, or bare up to whatever ends it.
static KEY_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    rule(r#"^(?:"[^"\n]*"|'[^'\n]*'|[^\s,;{}()\[\]<>"'`:][^\s,;{}()\[\]<>"'`]*)"#)
});

/// The name of `NAME=value`, as an environment variable or a query parameter,
/// or of `NAME = "value"` as a setting, and the equals sign. Capture 1 is the
/// name, 2 the equals sign. `NAME_VALUE` reads the value after it.
static NAME: LazyLock<Regex> =
    LazyLock::new(|| rule(r"\b([A-Za-z_][A-Za-z0-9_.-]*)([ \t]*=[ \t]*)"));

/// A value after an equals sign: quoted, or up to the next blank.
static NAME_VALUE: LazyLock<Regex> = LazyLock::new(|| rule(r#"^(?:"[^"\n]*"|'[^'\n]*'|\S+)"#));

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

/// The text being screened, and whether any rule has found a secret in it.
struct Screen {
    text: String,
    found: bool,
}

impl Screen {
    /// Replaces each match of `rule` that `decide` takes. `decide` is given
    /// the match and the text after it.
    fn step(&mut self, rule: &Regex, decide: &dyn Fn(&Captures, &str) -> Option<String>) {
        let mut hit = false;
        let out = rule.replace_all(&self.text, |caps: &Captures| {
            let whole = caps.get(0).expect("whole match");
            match decide(caps, &self.text[whole.end()..]) {
                Some(replacement) => {
                    hit = true;
                    replacement
                }
                None => whole.as_str().to_owned(),
            }
        });
        if hit {
            self.text = out.into_owned();
            self.found = true;
        }
    }

    /// Redacts the value of each `key sep value` in the text that `take` takes.
    /// `head` finds a key and its separator (capture 1 is the key) and `value`
    /// the value right after it. Only a secret-named key is read on, and its
    /// name is looked at before its value: a key that is not one is passed over
    /// by itself, so a pair that its value would have swallowed is still found
    /// (`INFO: password: x`, `limit=10&token=x`), and the value is not read.
    /// `take` is given the match of `head`, the value and the text after it, and
    /// returns how much of that text goes with the value, or `None` if the
    /// value is not a secret's.
    fn pairs(
        &mut self,
        head: &Regex,
        value: &Regex,
        take: &dyn Fn(&Captures, &str, &str) -> Option<usize>,
    ) {
        let text = &self.text;
        let (mut out, mut copied, mut at) = (String::new(), 0, 0);
        while let Some(caps) = head.captures_at(text, at) {
            let from = caps.get(0).expect("whole match").end();
            at = from;
            if !secret_named(&caps[1]) {
                continue;
            }
            let Some(found) = value.find(&text[from..]) else {
                continue;
            };
            let to = from + found.end();
            at = to;
            let Some(more) = take(&caps, found.as_str(), &text[to..]) else {
                continue;
            };
            out.push_str(&text[copied..from]);
            out.push_str(&mask(found.as_str()));
            copied = to + more;
            at = copied;
        }
        if copied > 0 {
            out.push_str(&text[copied..]);
            self.text = out;
            self.found = true;
        }
    }
}

/// Screens `s` for the turn-only memory: [`FilterResult::Keep`] if nothing was
/// found, [`FilterResult::Redacted`] with the secrets replaced, or
/// [`FilterResult::Drop`] if nothing but the secrets was there.
pub fn screen_for_memory(s: &str) -> FilterResult {
    let mut screen = Screen {
        text: s.to_owned(),
        found: false,
    };
    screen.step(&PRIVATE_KEY, &|_, _| Some(PLACEHOLDER.into()));
    screen.step(&PREFIXED, &|_, _| Some(PLACEHOLDER.into()));
    screen.pairs(&KEY, &KEY_VALUE, &|_, value, rest| {
        let code = unquote(value).0.is_empty() && (value.contains("::") || follows_code(rest));
        if code { None } else { takes(value, rest) }
    });
    screen.pairs(&NAME, &NAME_VALUE, &|caps, value, rest| {
        // Without quotes only `NAME=value` is an assignment; `a = b` is code.
        let spaced = caps[2].contains([' ', '\t']);
        if spaced && unquote(value).0.is_empty() {
            None
        } else {
            takes(value, rest)
        }
    });
    screen.step(&LONG_FLAG, &|caps, rest| {
        let value = &caps[3];
        let (quote, inner) = unquote(value);
        let reference = || {
            if quote.is_empty() {
                refers_to(value, rest)
            } else {
                is_reference(inner)
            }
        };
        (secret_named(&caps[1]) && !reference())
            .then(|| format!("--{}{}{}", &caps[1], &caps[2], mask(value)))
    });
    screen.step(&AUTH_SCHEME, &|caps, rest| {
        (!refers_to(&caps[2], rest)).then(|| format!("{}{PLACEHOLDER}", &caps[1]))
    });
    screen.step(&AUTH_VALUE, &|caps, rest| {
        let value = &caps[2];
        (value.bytes().any(|b| b.is_ascii_digit()) && !is_reference(value) && !follows_code(rest))
            .then(|| format!("{}{PLACEHOLDER}", &caps[1]))
    });
    screen.step(&BEARER, &|caps, rest| {
        let value = &caps[2];
        let credential = value.len() >= 20 || value.bytes().any(|b| b.is_ascii_digit());
        (credential && !is_reference(value) && !follows_code(rest))
            .then(|| format!("{}{PLACEHOLDER}", &caps[1]))
    });
    screen.step(&URL_PASSWORD, &|caps, _| {
        (&caps[2] != PLACEHOLDER).then(|| format!("://{}:{PLACEHOLDER}@", &caps[1]))
    });
    screen.step(&URL_TOKEN, &|caps, _| {
        (caps[1].chars().count() >= 16 && !is_reference(&caps[1]))
            .then(|| format!("://{PLACEHOLDER}@"))
    });
    screen.step(&RUN, &|caps, _| {
        looks_random(&caps[0]).then(|| PLACEHOLDER.into())
    });

    if !screen.found {
        return FilterResult::Keep(s.to_owned());
    }
    if nothing_useful_remains(&screen.text) {
        return FilterResult::Drop;
    }
    FilterResult::Redacted(screen.text)
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

/// A value that only points at a secret: empty, the mark of one already
/// redacted, a variable, a template, a placeholder, a YAML alias, anchor or tag
/// with a name, or a word for nothing. The whole value has the form, up to a
/// blank, a quote or the end, and `,`, `;`, `)` or `]` only close it at the end
/// of the value: a secret that merely starts like one (`!Passw0rd`,
/// `@dm1nSecret99`, `*secret*`, `<abc>def`, `$X,hunter2`) is still a secret.
fn is_reference(value: &str) -> bool {
    static FORM: LazyLock<Regex> = LazyLock::new(|| {
        rule(
            r#"^(?:\$\{[^}\n]*\}\}?|\$[A-Za-z_][A-Za-z0-9_]*|<[^>\n]*>|\{\{.*?\}\}|%\([^)\n]*\)s|%[A-Za-z_][A-Za-z0-9_]*%|[*&][A-Za-z0-9_][A-Za-z0-9_.-]*|![A-Za-z0-9_.-]*[ \t]+[A-Za-z_][A-Za-z0-9_.-]*|[|>])(?:[\s"'`]|[,;)\]]+(?:\s|$)|$)"#,
        )
    });
    let value = value.trim();
    value.is_empty()
        // Already redacted, with whatever punctuation a bare value took along.
        || value
            .strip_prefix(PLACEHOLDER)
            .is_some_and(|after| !after.starts_with(char::is_alphanumeric))
        || FORM.is_match(value)
        || ["null", "true", "false", "none", "nil", "undefined", "~"]
            .iter()
            .any(|word| value.eq_ignore_ascii_case(word))
}

/// [`is_reference`] for a bare `value` that `rest`, the text after it, may
/// complete: a template or a tag has blanks in it (`${{ secrets.TOKEN }}`,
/// `!secret db_password`), and the value is only its first word.
fn refers_to(value: &str, rest: &str) -> bool {
    is_reference(value)
        || (rest.starts_with([' ', '\t']) && is_reference(&format!("{value}{}", line_start(rest))))
}

/// How much of `rest`, the text after `value`, goes with the value when it is
/// a secret's, or `None` if it only refers to one or has no letter or digit in
/// it (the `&` of `&'a str`, `****`). A bare value that blanks and words follow
/// takes them to the end of the line (`correct horse battery staple`), unless
/// they are code (`mysql -e "select version()"` after `MYSQL_PWD=hunter2`):
/// then it is the value alone, as before.
fn takes(value: &str, rest: &str) -> Option<usize> {
    let (quote, inner) = unquote(value);
    if !inner.chars().any(char::is_alphanumeric) {
        return None;
    }
    if !quote.is_empty() {
        return (!is_reference(inner)).then_some(0);
    }
    if refers_to(value, rest) {
        return None;
    }
    if !rest.starts_with([' ', '\t']) || is_code(line_start(rest)) {
        return Some(0);
    }
    Some(rest.split('\n').next().unwrap_or("").trim_end().len())
}

/// The start of the line `rest` begins: its first `LINE_START_LEN` bytes at
/// most, which is as far as a value's words are read for what they are before
/// they are taken, so that a long line is not read again for each value on it.
// ponytail: code that first shows past this is taken for words and goes with the value;
// raise the length if a line comes out eaten.
fn line_start(rest: &str) -> &str {
    let mut end = rest.len().min(LINE_START_LEN);
    while !rest.is_char_boundary(end) {
        end -= 1;
    }
    rest[..end].split('\n').next().unwrap_or("")
}

/// Whether words after a value show themselves to be code: a path with `::`,
/// or an identifier followed by `(`, closed or not.
fn is_code(words: &str) -> bool {
    words.contains("::")
        || words
            .as_bytes()
            .windows(2)
            .any(|pair| pair[1] == b'(' && (pair[0].is_ascii_alphanumeric() || pair[0] == b'_'))
}

/// Whether what follows a bare value shows it to be code and not a value: a
/// call, a path, generics, a struct literal, a binding, or a field that ends
/// its line.
fn follows_code(rest: &str) -> bool {
    let line = rest.trim_start_matches([' ', '\t']);
    line.starts_with(['(', ')', '<', '{', '}', '[', ']', '='])
        || line.starts_with("::")
        // A `,` or `;` with nothing but blanks after it on the line. Reads no
        // further than the first thing that is not one.
        || line.strip_prefix([',', ';']).is_some_and(|after| {
            after
                .chars()
                .take_while(|&c| c != '\n')
                .all(char::is_whitespace)
        })
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

/// Whether a run of `[A-Za-z0-9+/_=-]` is a long random string. One with `=`
/// inside is a setting. The rest must hold a digit and a letter, not be a git
/// hash, and look random: hex by its spread, mixed case by its spread and by
/// not being made of words. A run with `_`, `/` or `-` in it is told by its
/// pieces between them (`pieces_look_random`).
fn looks_random(run: &str) -> bool {
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
    if token.contains(['_', '/', '-']) {
        return lower && upper && pieces_look_random(token);
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

/// Whether a mixed-case `token` with `_`, `/` or `-` in it, and a digit, is
/// random. An identifier, a path, a UUID or a version is made of words, numbers
/// and hashes between those characters (`Sources`, `PolarisSettings`,
/// `BootstrapV2Publisher`, `20241102`, a commit's hex) and a base64 key is not
/// (`wJalrXUtnFEMI`, `K7MDENG`, `bPxRfiCYEXAMPLEKEY`): it is random if the
/// pieces that are none of them are long enough together, and the whole is
/// spread as widely as a random string is.
fn pieces_look_random(token: &str) -> bool {
    // A word is a capital and two lowercase letters or more, three lowercase
    // letters or more, or two capitals or more. A number may have a capital
    // before it (`V2`) and one after (`2D`). A hash is seven hex digits or more,
    // in one case. A shorter run of letters is how a random string reads
    // between its `-`s.
    static PLAIN: LazyLock<Regex> = LazyLock::new(|| {
        rule(
            r"^(?:(?:[A-Z][a-z]{2,}|[a-z]{3,}|[A-Z]{2,}|[A-Z]?[0-9]+[A-Z]?)+|[0-9a-f]{7,}|[0-9A-F]{7,})$",
        )
    });
    let loose: usize = token
        .split(['_', '/', '-'])
        .filter(|piece| !PLAIN.is_match(piece))
        .map(str::len)
        .sum();
    loose >= RANDOM_LOOSE_MIN_LEN && entropy(token) >= 3.5
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
        // A value that only starts like a reference is not one.
        ("password: !Passw0rd", "Passw0rd"),
        ("token: @dm1nSecret99", "dm1nSecret99"),
        ("api_key: *realsecretvalue*", "realsecretvalue"),
        ("TOKEN=%s3cretValue99", "s3cretValue99"),
        ("PASSWORD=(hunter2hunter2)", "hunter2hunter2"),
        // A reference that a second value follows, after `,` or `;`, is not one.
        ("TOKEN=$X,hunter2value", "hunter2value"),
        ("PASSWORD=*alias,hunter2value", "hunter2value"),
        ("TOKEN=${X};hunter2value", "hunter2value"),
        ("PASSWORD=<abc>,hunter2value", "hunter2value"),
        ("PASSWORD=%NAME%;hunter2value", "hunter2value"),
        // A bare value with words after it takes them to the end of the line.
        (
            "password: correct horse battery staple",
            "horse battery staple",
        ),
        ("PASSWORD=my secret phrase", "secret phrase"),
        // A pair that is not a secret's before one that is.
        ("INFO: password: hunter2", "hunter2"),
        ("Error: token: abc123xyz", "abc123xyz"),
        ("Warning: api_key: abcd1234efgh", "abcd1234efgh"),
        ("?limit=10&api_key=abcd1234efgh", "abcd1234efgh"),
        ("-d user=bob&password=hunter2", "hunter2"),
        ("?b=1&token=abc123xyz", "abc123xyz"),
        ("export A=b;API_KEY=x123456789", "x123456789"),
        ("sort_key=1&password=hunter2", "hunter2"),
        ("monkey: password: hunter2", "hunter2"),
        ("?max_tokens=100&api_key=abcd1234efgh", "abcd1234efgh"),
        // A long random string with `/`, `+`, `-` or `_` in it.
        (
            "AWS_SECRET wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY ok",
            "bPxRfiCYEXAMPLEKEY",
        ),
        (
            "a Zm9vYmFyYmF6cXV4LWl0LWlzLWE0LWxvbmctc2VjcmV0 b",
            "Zm9vYmFyYmF6cXV4LWl0",
        ),
        (
            "key qZ8x-Lw3_Tn7vKp2Rb9mYd4Hs6Jc1AeF end",
            "Tn7vKp2Rb9mYd4Hs6Jc1AeF",
        ),
        ("see Xk3+9QzLw/7vTnB2mPa+eR5yUc8dHs1J== here", "7vTnB2mPa"),
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
            // The verdict too: a text that came back redacted is not a find.
            assert!(
                matches!(screen_for_memory(&once), FilterResult::Keep(_)),
                "{once:?}"
            );
        }
    }

    #[test]
    fn a_secret_goes_with_the_words_after_it_and_not_with_the_pairs_around_it() {
        // A bare value takes the words after it, to the end of the line.
        assert_eq!(
            screened("password: correct horse battery staple"),
            "password: [REDACTED]"
        );
        assert_eq!(
            screened("PASSWORD=my secret phrase\nnext: line"),
            "PASSWORD=[REDACTED]\nnext: line"
        );
        // Code after it is not words: the value alone goes.
        assert_eq!(
            screened("MYSQL_PWD=hunter2 mysql -e \"select version()\""),
            "MYSQL_PWD=[REDACTED] mysql -e \"select version()\""
        );
        assert_eq!(
            screened("password: hunter2 Secret::new()"),
            "password: [REDACTED] Secret::new()"
        );
        // A quoted value, and one a delimiter ends, take no more.
        assert_eq!(
            screened("password: \"hunter2\" and then some"),
            "password: \"[REDACTED]\" and then some"
        );
        // A pair that is not a secret's before one that is, and after it.
        assert_eq!(
            screened("INFO: password: hunter2"),
            "INFO: password: [REDACTED]"
        );
        assert_eq!(
            screened("?limit=10&api_key=abcd1234efgh"),
            "?limit=10&api_key=[REDACTED]"
        );
        assert_eq!(
            screened("export A=b;API_KEY=x123456789"),
            "export A=b;API_KEY=[REDACTED]"
        );
        assert_eq!(
            screened("user: bob, password: hunter2, level: 3"),
            "user: bob, password: [REDACTED], level: 3"
        );
        // A name that has a secret's word but is not one does not hide it.
        assert_eq!(
            screened("sort_key=1&password=hunter2"),
            "sort_key=1&password=[REDACTED]"
        );
    }

    #[test]
    fn a_value_is_a_reference_only_when_all_of_it_has_the_form() {
        for reference in [
            "",
            "[REDACTED]",
            "[REDACTED];",
            "[REDACTED])",
            "${DB_PASSWORD}",
            "${{ secrets.TOKEN }}",
            "$DB_PASSWORD",
            "<your token>",
            "{{ vault.secret }}",
            "%(name)s",
            "%NAME%",
            "*alias",
            "&anchor",
            "!secret db_password",
            "|",
            ">",
            "null",
            "~",
            // Closing punctuation at the end of the value.
            "$X,",
            "${X});",
            "<abc>]",
        ] {
            assert!(is_reference(reference), "{reference:?}");
        }
        for secret in [
            "!Passw0rd",
            "@dm1nSecret99",
            "*realsecretvalue*",
            "%s3cret",
            "(hunter2)",
            "[hunter2]",
            "[REDACTED]hunter2",
            "{hunter2}",
            "`hunter2`",
            "$(cat file)",
            "<abc>def",
            "|hunter2",
            ">hunter2",
            // A second value after the closing punctuation.
            "$X,hunter2value",
            "*alias,hunter2value",
            "${X};hunter2value",
            "<abc>,hunter2value",
            "%NAME%;hunter2value",
        ] {
            assert!(!is_reference(secret), "{secret:?}");
        }
        // A template or a tag has blanks in it: the value is its first word,
        // and the words after it complete the form, comment or quote or not.
        assert!(refers_to("${{", " secrets.TOKEN }} # note"));
        assert!(refers_to("!secret", " db_password # note"));
        assert!(!refers_to("hunter2", " secrets.TOKEN }}"));
        assert!(!refers_to("!Passw0rd", ""));
        assert!(!refers_to("${{", ""));
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
        // A value with no letter or digit in it hides nothing.
        "token: &'a str,",
        "password: ********",
        "TOKEN=$OTHER",
        "TOKEN=$OTHER;",
        "GITHUB_TOKEN=${{ secrets.GITHUB_TOKEN }}",
        "the bearer token is required",
        "docker run -u 1000:1000 img",
        "ssh://git@github.com/user/repo",
        "http://localhost:8080/foo@bar",
        "sk-learn is a library",
        "token = get_token()",
        // References: a whole value of the form, with words in it too.
        "password: !secret db_password",
        "token: <your token>",
        "secret: {{ vault.secret }}",
        "SECRET={{ vault.secret }}",
        "api_key: &default_key",
        "api_key: *default_key",
        "password: |",
        "PASSWORD=%(db_password)s",
        "PASSWORD=%DB_PASSWORD%",
        "TOKEN=<your token>",
        "GITHUB_TOKEN=${{ secrets.GITHUB_TOKEN }} # set by the workflow",
        "curl -H \"Authorization: Bearer ${{ secrets.GITHUB_TOKEN }}\" https://api.github.com",
        "docker login --password ${{ secrets.REGISTRY_TOKEN }} ghcr.io",
        // A ledger line the screen has been through already.
        "asked=set API_TOKEN=[REDACTED]; answered=answer; src=c0-c1",
        // Prose with no separator after the name, code, and pairs that are
        // not a secret's.
        "the password is required",
        "INFO: user: bob and level: 3",
        "Error: tokens: 5, key: value",
        "?limit=10&sort_key=name&page=2",
        "password: hash(raw) or die",
        "secret: Secret::from(value) ok",
        "let password = rpassword::prompt_password(\"Password: \")?;",
        // Pieces that are words and numbers: kebab-case, a version, a UUID in
        // capitals, a path with a mix of cases and a number.
        "turn-only-memory-screen-for-the-ledger-lines",
        "2026-11-02-memory-1-safety-and-auto-compaction",
        "550E8400-E29B-41D4-A716-446655440000",
        "Sources/PolarisSettings/Bootstrap/V2Publisher9",
        "HTTP_Status_Code_Of_The_Last_Request_V2",
        // A hash is a piece too: a commit's hex in a path.
        "see https://raw.githubusercontent.com/polaris-core/polaris/8f3a1c2e9b7d4560112233445566778899aabbc0/README.md",
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
            // Base64 with `/` and `+`, base64url with `-` and `_`.
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            "Xk3+9QzLw/7vTnB2mPa+eR5yUc8dHs1J==",
            "qZ8x-Lw3_Tn7vKp2Rb9mYd4Hs6Jc1AeF",
            "E9Melhoa2OwvFr-EMTJguCHaoeK1t8URW_buGJSstw",
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
            // Pieces of words and numbers: a version, kebab-case, a path, a
            // name in capitals and one that mixes cases.
            "2026-11-02-memory-1-safety-and-auto-compaction",
            "turn-only-memory-screen-for-the-ledger-lines",
            "crates/polaris-core/src/turn_memory/screen_tests",
            "WORKFLOW_ARTIFACT_BODIES_SNAPSHOT_V1_2",
            "HTTP_Status_Code_Of_The_Last_Request_V2",
            "Sources/PolarisSettings/Bootstrap/V2Publisher9",
            "polaris-core/polaris/8f3a1c2e9b7d4560112233445566778899aabbc0/README",
            // Names whose words are short: `dh`, `DH`, `2D`, `ID3D12`.
            "NID_dhSinglePass_stdDH_sha224kdf_scheme",
            "org/en-US/docs/Web/API/CanvasRenderingContext2D/rotate",
            "IID_ID3D12CommandAllocator",
        ] {
            assert!(!looks_random(not_random), "{not_random}");
        }
    }
}
