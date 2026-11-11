//! Discord's rate limits, from the headers it sends. Pure: every method takes `now`.
//!
//! - **Buckets.** Each reply names its bucket (`X-RateLimit-Bucket`) and says how many calls
//!   are left (`-Remaining`) and when the bucket refills (`-Reset-After`, seconds). A bucket
//!   is shared by routes with the same hash, separately per major parameter (channel, guild,
//!   webhook). Before a call: if the bucket is empty and not yet refilled, wait.
//! - **Global.** A 429 with `X-RateLimit-Global: true` (or `"global": true`) stops every
//!   call that counts against the global limit until `retry_after`. Interaction and
//!   webhook-token routes do not count.
//! - **429.** Wait `retry_after` (the body's seconds; the `Retry-After` header as a fallback),
//!   then try again.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use reqwest::header::HeaderMap;
use serde_json::Value;

use crate::routes::Route;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Bucket {
    remaining: u32,
    reset_at: Instant,
}

#[derive(Default, Debug)]
pub struct RateLimiter {
    /// Route key → bucket hash, learned from replies.
    hashes: HashMap<String, String>,
    /// "hash:major" (or "route:major" before the hash is known) → bucket.
    buckets: HashMap<String, Bucket>,
    global_until: Option<Instant>,
}

fn header<'a>(h: &'a HeaderMap, name: &str) -> Option<&'a str> {
    h.get(name).and_then(|v| v.to_str().ok())
}

fn seconds(s: f64) -> Duration {
    Duration::from_secs_f64(s.max(0.0))
}

impl RateLimiter {
    fn bucket_key(&self, route: &Route) -> String {
        let id = self.hashes.get(&route.key).cloned().unwrap_or_else(|| route.key.clone());
        format!("{id}:{}", route.major)
    }

    /// Before a call: `None` to go now (and one call is taken from the bucket), or how long
    /// to wait first (then ask again).
    pub fn acquire(&mut self, route: &Route, now: Instant) -> Option<Duration> {
        if route.global {
            if let Some(until) = self.global_until {
                if until > now {
                    return Some(until - now);
                }
                self.global_until = None;
            }
        }
        let key = self.bucket_key(route);
        if let Some(b) = self.buckets.get_mut(&key) {
            if b.reset_at <= now {
                self.buckets.remove(&key);
            } else if b.remaining == 0 {
                return Some(b.reset_at - now);
            } else {
                b.remaining -= 1;
            }
        }
        None
    }

    /// After any reply but a 429: learn the bucket from its headers.
    pub fn record(&mut self, route: &Route, headers: &HeaderMap, now: Instant) {
        if let Some(hash) = header(headers, "x-ratelimit-bucket") {
            self.hashes.insert(route.key.clone(), hash.to_owned());
        }
        let remaining = header(headers, "x-ratelimit-remaining").and_then(|v| v.parse::<u32>().ok());
        let reset_after = header(headers, "x-ratelimit-reset-after").and_then(|v| v.parse::<f64>().ok());
        if let (Some(remaining), Some(reset_after)) = (remaining, reset_after) {
            let key = self.bucket_key(route);
            self.buckets.insert(key, Bucket { remaining, reset_at: now + seconds(reset_after) });
        }
    }

    /// After a 429: how long to wait before trying again. A global 429 stops every
    /// global-limited route; a bucket 429 empties that bucket until then.
    pub fn on_429(&mut self, route: &Route, headers: &HeaderMap, body: &Value, now: Instant) -> Duration {
        let retry = body
            .get("retry_after")
            .and_then(Value::as_f64)
            .or_else(|| header(headers, "retry-after").and_then(|v| v.parse().ok()))
            .map(seconds)
            .unwrap_or(Duration::from_secs(1));
        let global = header(headers, "x-ratelimit-global").is_some_and(|v| v.eq_ignore_ascii_case("true"))
            || body.get("global").and_then(Value::as_bool).unwrap_or(false);
        if global {
            self.global_until = Some(now + retry);
        } else {
            if let Some(hash) = header(headers, "x-ratelimit-bucket") {
                self.hashes.insert(route.key.clone(), hash.to_owned());
            }
            let key = self.bucket_key(route);
            self.buckets.insert(key, Bucket { remaining: 0, reset_at: now + retry });
        }
        retry
    }
}
