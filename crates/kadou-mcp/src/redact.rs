//! Secret-value redaction (`docs/design/05-prd.md` §6.6): applied to a run's output stream
//! before it is written to the history log or returned in the MCP result, so both see the
//! same already-redacted text.
//!
//! §6.6 lists four encodings to redact per secret value: the literal value, its standard
//! base64 encoding, base64 of `user:value`, and its URL-encoded form. A minimum length of 8
//! characters keeps short values from shredding unrelated output.

use base64::Engine as _;

const MIN_SECRET_LEN: usize = 8;
const REDACTED: &str = "****";

/// Replaces every occurrence of every secret in `secrets` (and its base64/URL-encoded forms)
/// with `****`, plus every `base64("a:b")` HTTP-basic-auth pairing over ordered pairs drawn
/// from `need_values` (§6.6's fourth encoding, I-18) — a kata doing
/// `curl -u "$JENKINS_USER:$JENKINS_TOKEN"` with `curl -v` echoes exactly this pairing.
/// `need_values` is every resolved need's value, secret or not: the plain half of a pairing
/// (e.g. `jenkins_user`) is never redacted on its own, only as part of the combined pairing —
/// no generic "username" concept is needed, just the cross product.
pub fn redact_all(text: &str, secrets: &[String], need_values: &[String]) -> String {
    let mut out = text.to_string();
    for secret in secrets {
        if secret.chars().count() < MIN_SECRET_LEN {
            continue;
        }
        for variant in variants(secret) {
            if variant.is_empty() {
                continue;
            }
            out = out.replace(&variant, REDACTED);
        }
    }
    for pair in basic_auth_variants(need_values) {
        out = out.replace(&pair, REDACTED);
    }
    out
}

/// `base64("a:b")` for every ordered pair of distinct values in `values` whose combined
/// `"a:b"` is at least [`MIN_SECRET_LEN`] characters.
fn basic_auth_variants(values: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for a in values {
        for b in values {
            if a == b {
                continue;
            }
            let pair = format!("{a}:{b}");
            if pair.chars().count() < MIN_SECRET_LEN {
                continue;
            }
            out.push(base64::engine::general_purpose::STANDARD.encode(pair.as_bytes()));
        }
    }
    out
}

fn variants(secret: &str) -> Vec<String> {
    vec![
        secret.to_string(),
        base64::engine::general_purpose::STANDARD.encode(secret.as_bytes()),
        percent_encode(secret),
    ]
}

/// A minimal RFC 3986 percent-encoder (unreserved: alnum, `-`, `_`, `.`, `~`). No crate
/// dependency needed for this narrow use.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_the_literal_secret_value() {
        let out = redact_all("token=hunter2ok", &["hunter2ok".to_string()], &[]);
        assert_eq!(out, "token=****");
    }

    #[test]
    fn short_values_are_never_redacted() {
        let out = redact_all("port=8080", &["8080".to_string()], &[]);
        assert_eq!(out, "port=8080");
    }

    #[test]
    fn redacts_base64_and_url_encoded_forms() {
        let secret = "hunter2ok!";
        let b64 = base64::engine::general_purpose::STANDARD.encode(secret.as_bytes());
        let url = percent_encode(secret);
        let text = format!("raw={secret} b64={b64} url={url}");
        let out = redact_all(&text, &[secret.to_string()], &[]);
        assert_eq!(out, "raw=**** b64=**** url=****");
    }

    #[test]
    fn multiple_secrets_are_all_redacted() {
        let out = redact_all(
            "a=secretvalue1 b=secretvalue2",
            &["secretvalue1".to_string(), "secretvalue2".to_string()],
            &[],
        );
        assert_eq!(out, "a=**** b=****");
    }

    #[test]
    fn redacts_a_basic_auth_header_built_from_two_needs() {
        // §6.6's fourth encoding: base64("user:value"). jenkins_user is plain (not itself a
        // secret, so it isn't in `secrets`) but a kata doing
        // `curl -u "$JENKINS_USER:$JENKINS_TOKEN"` with `curl -v` echoes exactly this pairing
        // (I-18) -- needs no "username" concept, just the cross product of resolved need
        // values.
        let user = "ci-user".to_string();
        let token = "hunter2-token".to_string();
        let basic =
            base64::engine::general_purpose::STANDARD.encode(format!("{user}:{token}").as_bytes());
        let text = format!("Authorization: Basic {basic}");
        let out = redact_all(&text, &[token.clone()], &[user, token]);
        assert_eq!(out, "Authorization: Basic ****");
    }
}
