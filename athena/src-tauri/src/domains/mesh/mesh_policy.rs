//! Who ATHENA listens to over Meshtastic, and how it proves who they are.
//!
//! The mesh is shared with strangers: the default channel's key is public, and anyone can set
//! their radio to claim any node number. Members reach the station from afar, so the rules are:
//!
//! * **Commands only in encrypted direct messages.** Meshtastic 2.5 and later encrypts a direct
//!   message to the receiving radio's public key (PKC), so only this station can read it and the
//!   sender's key proves which radio sent it. Station commands, the AI, operator messages and
//!   radio linking are accepted only that way. Broadcasts and channel messages never run
//!   commands.
//! * **A radio is pinned to its key.** Linking a radio records its public key. A later message
//!   claiming that node number with another key is refused and the operator is told.
//! * **Links expire.** A linked radio must re-enter the PIN after a set number of days, so a lost
//!   radio stops working on its own.
//! * **PIN guessing is stopped.** Five wrong PINs from a radio, or for a name, lock both out for
//!   30 minutes.
//! * **Strangers get at most one hint a day.** Nothing is sent in reply to a broadcast.
//! * **Station traffic stays on the ATHENA channel.** Logbook, calendar and map sync go out only
//!   on a private channel with its own key, and sync arriving on any other channel is ignored.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Mutex, OnceLock};

pub const BROADCAST: u32 = 0xFFFF_FFFF;
/// Days a radio link lasts before the PIN must be entered again.
pub const DEFAULT_LINK_DAYS: u32 = 30;
pub const PIN_FAILURES_BEFORE_LOCK: u32 = 5;
pub const PIN_FAILURE_WINDOW_SECS: i64 = 15 * 60;
pub const PIN_LOCK_SECS: i64 = 30 * 60;
/// How often a stranger or an unencrypted sender may get a hint.
pub const HINT_EVERY_SECS: i64 = 24 * 3600;

/// The private ATHENA channel index on this radio, or -1 when none is set up.
static ATHENA_CHANNEL: AtomicI32 = AtomicI32::new(-1);

pub fn athena_channel() -> Option<u32> {
    u32::try_from(ATHENA_CHANNEL.load(Ordering::Relaxed)).ok().filter(|c| (1..=7).contains(c))
}

pub fn set_athena_channel(ch: Option<u32>) {
    ATHENA_CHANNEL.store(ch.filter(|c| (1..=7).contains(c)).map_or(-1, |c| c as i32), Ordering::Relaxed);
}

/// Latest channel use reported by our own radio, percent, as f32 bits (NaN when unknown).
static CHANNEL_UTIL_BITS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x7fc0_0000);

pub fn set_channel_utilization(pct: f32) {
    CHANNEL_UTIL_BITS.store(pct.to_bits(), Ordering::Relaxed);
}

pub fn channel_utilization() -> Option<f32> {
    let v = f32::from_bits(CHANNEL_UTIL_BITS.load(Ordering::Relaxed));
    v.is_finite().then_some(v)
}

/// True for distress traffic, which is never held back.
pub fn is_distress(text: &str) -> bool {
    text.trim_start().to_ascii_uppercase().starts_with("SOS")
}

/// The channel station sync goes out on, or why it cannot go out.
pub fn sync_channel() -> Result<u32, String> {
    athena_channel().ok_or_else(|| {
        "Set up the private ATHENA channel first (Comms, Radio, ATHENA channel). Station records are never sent on the public channel.".to_string()
    })
}

/// True for station sync (logbook, calendar, map reports) arriving where it should.
pub fn accept_sync(channel: u32, to: u32) -> bool {
    to == BROADCAST && athena_channel() == Some(channel)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// Not for this station, or our own echo.
    Ignore,
    /// A direct message that only this station could read: act on it.
    Secure,
    /// A direct message on a channel key others may hold. Do not act; hint at most once a day.
    Unencrypted,
}

pub fn disposition(from: u32, to: u32, my: u32, pki_encrypted: bool) -> Disposition {
    if my == 0 || from == my || to != my {
        return Disposition::Ignore;
    }
    if pki_encrypted {
        Disposition::Secure
    } else {
        Disposition::Unencrypted
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub public_key: Vec<u8>,
    /// Unix seconds when the PIN was last entered.
    pub verified_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStatus {
    Linked,
    NotLinked,
    /// The node number is linked, but this message came from another key.
    KeyMismatch,
    Expired,
}

pub fn link_status(stored: Option<&Link>, key: &[u8], now: i64, days: u32) -> LinkStatus {
    let Some(link) = stored else { return LinkStatus::NotLinked };
    if link.public_key.is_empty() || key.is_empty() || link.public_key != key {
        return LinkStatus::KeyMismatch;
    }
    if now - link.verified_at > i64::from(days.max(1)) * 86_400 {
        return LinkStatus::Expired;
    }
    LinkStatus::Linked
}

/// Remembers when something last happened per key, to send something at most once a period.
#[derive(Default)]
pub struct RateGate {
    last: HashMap<String, i64>,
}

impl RateGate {
    pub fn allow(&mut self, key: &str, now: i64, every_secs: i64) -> bool {
        if self.last.len() > 4096 {
            self.last.retain(|_, t| now - *t < every_secs);
        }
        match self.last.get(key) {
            Some(t) if now - *t < every_secs => false,
            _ => {
                self.last.insert(key.to_string(), now);
                true
            }
        }
    }
}

/// Wrong-PIN counting per radio and per name.
#[derive(Default)]
pub struct PinGuard {
    failures: HashMap<String, (u32, i64)>,
    locked_until: HashMap<String, i64>,
}

impl PinGuard {
    /// Seconds left on a lock covering any of these keys.
    pub fn locked_for(&self, keys: &[String], now: i64) -> Option<i64> {
        keys.iter().filter_map(|k| self.locked_until.get(k)).map(|until| until - now).filter(|s| *s > 0).max()
    }

    /// Count a wrong PIN. True when this failure starts a lock.
    pub fn fail(&mut self, keys: &[String], now: i64) -> bool {
        let mut locked = false;
        for k in keys {
            let e = self.failures.entry(k.clone()).or_insert((0, now));
            if now - e.1 > PIN_FAILURE_WINDOW_SECS {
                *e = (0, now);
            }
            e.0 += 1;
            if e.0 >= PIN_FAILURES_BEFORE_LOCK {
                self.locked_until.insert(k.clone(), now + PIN_LOCK_SECS);
                self.failures.remove(k);
                locked = true;
            }
        }
        locked
    }

    pub fn succeed(&mut self, keys: &[String]) {
        for k in keys {
            self.failures.remove(k);
        }
    }
}

pub fn pin_keys(node: u32, name: &str) -> Vec<String> {
    vec![format!("node:{node}"), format!("name:{}", name.trim().to_lowercase())]
}

pub fn hints() -> &'static Mutex<RateGate> {
    static G: OnceLock<Mutex<RateGate>> = OnceLock::new();
    G.get_or_init(|| Mutex::new(RateGate::default()))
}

pub fn pins() -> &'static Mutex<PinGuard> {
    static G: OnceLock<Mutex<PinGuard>> = OnceLock::new();
    G.get_or_init(|| Mutex::new(PinGuard::default()))
}

/// Spacing between packets this station sends. LongFast carries roughly one short packet every
/// couple of seconds; a busy channel gets more room.
pub fn send_gap_ms(channel_utilization_pct: Option<f32>) -> u64 {
    match channel_utilization_pct {
        Some(u) if u >= 40.0 => 8_000,
        Some(u) if u >= 25.0 => 5_000,
        _ => 2_500,
    }
}

/// Short form of a radio key, for the operator to compare by eye.
pub fn key_fingerprint(key: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    if key.is_empty() {
        return "none".to_string();
    }
    let h = Sha256::digest(key);
    h.iter().take(4).map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_encrypted_direct_messages_are_acted_on() {
        let me = 100;
        assert_eq!(disposition(5, me, me, true), Disposition::Secure);
        assert_eq!(disposition(5, me, me, false), Disposition::Unencrypted);
        assert_eq!(disposition(5, BROADCAST, me, true), Disposition::Ignore, "broadcasts never run commands");
        assert_eq!(disposition(5, 7, me, true), Disposition::Ignore, "someone else's message");
        assert_eq!(disposition(me, me, me, true), Disposition::Ignore, "our own echo");
        assert_eq!(disposition(5, 0, 0, true), Disposition::Ignore, "radio not configured yet");
    }

    #[test]
    fn links_are_pinned_to_the_radio_key_and_expire() {
        let link = Link { public_key: vec![1, 2, 3], verified_at: 1_000 };
        assert_eq!(link_status(Some(&link), &[1, 2, 3], 1_000 + 86_400, 30), LinkStatus::Linked);
        assert_eq!(link_status(Some(&link), &[9, 9, 9], 1_000, 30), LinkStatus::KeyMismatch);
        assert_eq!(link_status(Some(&link), &[], 1_000, 30), LinkStatus::KeyMismatch);
        assert_eq!(link_status(Some(&link), &[1, 2, 3], 1_000 + 31 * 86_400, 30), LinkStatus::Expired);
        assert_eq!(link_status(None, &[1, 2, 3], 0, 30), LinkStatus::NotLinked);
    }

    #[test]
    fn pin_guessing_locks_the_radio_and_the_name() {
        let mut g = PinGuard::default();
        let keys = pin_keys(5, "Developer");
        for _ in 0..4 {
            assert!(!g.fail(&keys, 0));
        }
        assert_eq!(g.locked_for(&keys, 0), None);
        assert!(g.fail(&keys, 10));
        assert!(g.locked_for(&keys, 10).is_some());
        // Another radio trying the same name is locked too.
        assert!(g.locked_for(&pin_keys(6, "operator"), 20).is_some());
        assert_eq!(g.locked_for(&keys, 10 + PIN_LOCK_SECS), None);
        // Failures spread out beyond the window do not add up.
        let mut g = PinGuard::default();
        for i in 0..10 {
            assert!(!g.fail(&keys, i * (PIN_FAILURE_WINDOW_SECS + 1)));
        }
    }

    #[test]
    fn hints_go_out_once_a_day() {
        let mut g = RateGate::default();
        assert!(g.allow("n5", 0, HINT_EVERY_SECS));
        assert!(!g.allow("n5", 60, HINT_EVERY_SECS));
        assert!(g.allow("n6", 60, HINT_EVERY_SECS));
        assert!(g.allow("n5", HINT_EVERY_SECS, HINT_EVERY_SECS));
    }

    #[test]
    fn sync_needs_the_private_channel() {
        set_athena_channel(None);
        assert!(sync_channel().is_err());
        assert!(!accept_sync(0, BROADCAST));
        set_athena_channel(Some(1));
        assert_eq!(sync_channel(), Ok(1));
        assert!(accept_sync(1, BROADCAST));
        assert!(!accept_sync(0, BROADCAST), "public channel");
        assert!(!accept_sync(1, 42), "direct message");
        set_athena_channel(Some(0));
        assert_eq!(athena_channel(), None, "channel 0 is the public one");
        set_athena_channel(None);
    }

    #[test]
    fn busy_channels_get_more_room() {
        assert!(send_gap_ms(Some(50.0)) > send_gap_ms(Some(30.0)));
        assert!(send_gap_ms(Some(30.0)) > send_gap_ms(None));
    }
}
