//! Tiny duration parser shared by the security helpers.
//!
//! Roadmap §1.10 — `secret_rotation` cadence / overlap and any
//! header sub-block durations (HSTS `max_age` excepted; that one is
//! already a numeric token) are authored as `<digits><unit>`
//! literals. Doctor needs to compare them numerically (the
//! `secret-rotation-overlap-contract` rule rejects overlap >
//! cadence). The literal stays verbatim in the IR; this module is
//! the canonical lift to seconds.
//!
//! Closed unit catalog mirrors `lazuli_cli::doctor::poller::tick_too_fast_001`
//! (`s` / `m` / `h` / `d`).

/// Parse a duration literal like `90d` / `24h` / `0h` into seconds.
/// Returns `None` when the literal is empty, has no unit, has an
/// unknown unit, or overflows.
///
/// ## Examples
///
/// ```
/// use lazuli_ir::security_duration::duration_seconds;
///
/// assert_eq!(duration_seconds("30s"), Some(30));
/// assert_eq!(duration_seconds("24h"), Some(24 * 60 * 60));
/// assert_eq!(duration_seconds("1y"), None); // unknown unit
/// ```
pub fn duration_seconds(raw: &str) -> Option<u64> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let unit_start = raw.find(|c: char| !c.is_ascii_digit())?;
    if unit_start == 0 {
        return None;
    }
    let (digits, unit) = raw.split_at(unit_start);
    let n = digits.parse::<u64>().ok()?;
    match unit {
        "s" => Some(n),
        "m" => n.checked_mul(60),
        "h" => n.checked_mul(60 * 60),
        "d" => n.checked_mul(24 * 60 * 60),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_seconds() {
        assert_eq!(duration_seconds("30s"), Some(30));
    }

    #[test]
    fn parses_minutes() {
        assert_eq!(duration_seconds("5m"), Some(300));
    }

    #[test]
    fn parses_hours() {
        assert_eq!(duration_seconds("24h"), Some(24 * 60 * 60));
    }

    #[test]
    fn parses_days() {
        assert_eq!(duration_seconds("90d"), Some(90 * 24 * 60 * 60));
    }

    #[test]
    fn parses_zero() {
        assert_eq!(duration_seconds("0h"), Some(0));
    }

    #[test]
    fn rejects_no_unit() {
        assert!(duration_seconds("123").is_none());
    }

    #[test]
    fn rejects_empty() {
        assert!(duration_seconds("").is_none());
    }

    #[test]
    fn rejects_unknown_unit() {
        assert!(duration_seconds("1y").is_none());
    }
}
