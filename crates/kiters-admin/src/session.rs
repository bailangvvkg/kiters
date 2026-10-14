//! Sessions: one password, one signed cookie, nothing stored.
//!
//! The cookie is `kiters_admin=<hex hmac>.<expiry>`: keyed by the
//! password itself, verified in constant time, good until its expiry.
//! There is no session table to lose and no logout that has to reach
//! anyone — the signature is the session.

use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

const COOKIE: &str = "kiters_admin";
const TTL_SECONDS: i64 = 12 * 60 * 60;

type HmacSha256 = Hmac<Sha256>;

/// The one password, keyed for signing.
pub struct Sessions {
    key: Vec<u8>,
}

impl Sessions {
    /// Sessions signed by this password.
    pub fn new(password: &str) -> Self {
        Sessions { key: Sha256::digest(password.as_bytes()).to_vec() }
    }

    /// Whether this is the password. Constant time: a wrong answer
    /// takes as long as a right one.
    pub fn password_matches(&self, candidate: &str) -> bool {
        same(&self.key, Sha256::digest(candidate.as_bytes()).as_slice())
    }

    /// A cookie for a login now.
    pub fn cookie(&self) -> String {
        let exp = chrono::Utc::now().timestamp() + TTL_SECONDS;
        format!("{COOKIE}={}; Path=/; HttpOnly; SameSite=Strict; Max-Age={TTL_SECONDS}", self.token(exp))
    }

    /// The cookie that ends a session: same name, nothing in it.
    pub fn gone_cookie(&self) -> String {
        format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0")
    }

    fn token(&self, exp: i64) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("hmac takes any key");
        mac.update(exp.to_string().as_bytes());
        format!("{}.{}", hex(&mac.finalize().into_bytes()), exp)
    }

    /// Whether a request's cookies carry a session that still stands.
    pub fn verify(&self, cookies: &str) -> bool {
        for pair in cookies.split(';') {
            let pair = pair.trim();
            let Some(value) = pair.strip_prefix(&format!("{COOKIE}=")) else { continue };
            let Some((_mac, exp)) = value.split_once('.') else { continue };
            let Ok(exp) = exp.parse::<i64>() else { continue };
            if chrono::Utc::now().timestamp() > exp {
                continue;
            }
            // Same length, then every byte folded: no early exit for
            // anyone to listen for.
            if !same(&self.token(exp), value) {
                return false;
            }
            return true;
        }
        false
    }
}

/// Equal in constant time: lengths first, then every byte folded.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
