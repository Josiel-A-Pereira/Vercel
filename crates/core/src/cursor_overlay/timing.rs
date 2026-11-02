/// How long a platform renderer may take to report that the cursor reached its
/// destination before the action dispatches anyway.
pub const CURSOR_ARRIVAL_TIMEOUT_MS: u64 = 900;

/// The largest cursor travel plus dwell that still leaves room for the socket
/// round trip and frame pacing inside the arrival timeout.
pub const CURSOR_MOTION_BUDGET_MS: u64 = CURSOR_ARRIVAL_TIMEOUT_MS - 200;

/// How long the overlay stays visible with no instruction before it rests.
pub const CURSOR_IDLE_REST_MS: u64 = 6_000;

/// How long the outline around a clicked element stays on screen.
pub const CURSOR_HIGHLIGHT_HOLD_MS: u64 = 900;

/// How long the overlay takes to fade out once it rests.
///
/// The macOS bridge steps its window alpha down in thirteen 12ms hops, which
/// is where this number comes from. It is stated here so both renderers can
/// name one value rather than each carrying its own; the bridge still
/// hardcodes its own copy in Objective-C and consuming this is a promotion
/// item, not a change either platform can make alone.
pub const CURSOR_REST_FADE_MS: u64 = 156;

/// How long the label card takes to appear once its text changes.
///
/// macOS reveals it with a 0.18s opacity fade and an ease-out scale pop, so a card that
/// arrives does not simply blink into place. The same caveat applies: the
/// bridge holds its own copy for now.
pub const CURSOR_LABEL_REVEAL_MS: u64 = 180;

/// Whether a cursor that has been quiet for `quiet` should rest under a rest
/// time of `rest_ms`. A rest time of zero keeps the cursor until its session
/// ends or the overlay is disabled.
pub fn rest_is_due(rest_ms: u64, quiet: std::time::Duration) -> bool {
    rest_ms != 0 && quiet.as_millis() >= u128::from(rest_ms)
}

/// How long a renderer that polls for rest may wait before its next check:
/// the time left until the rest is due, never more than `most`. Waiting a
/// fixed tick instead would let a short rest time (e.g. 500 ms) fire up to a
/// whole tick late. A rest time of zero never comes due, so it waits `most`.
pub fn rest_wait(
    rest_ms: u64,
    quiet: std::time::Duration,
    most: std::time::Duration,
) -> std::time::Duration {
    if rest_ms == 0 {
        return most;
    }
    std::time::Duration::from_millis(rest_ms)
        .saturating_sub(quiet)
        .min(most)
}
