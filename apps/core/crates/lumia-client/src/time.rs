//! The clock and the timer, per target. Natively they are the system
//! clock and tokio's timer; on `wasm32-unknown-unknown` the system clock
//! traps and there is no runtime, so they are `Date.now()` and a
//! `setTimeout` future.

use std::time::Duration;

/// Microseconds since the Unix epoch.
#[must_use]
pub fn now_micros() -> i64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_micros()).unwrap_or(i64::MAX))
            .unwrap_or(0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        // Milliseconds with a fractional part, to microseconds.
        #[allow(clippy::cast_possible_truncation)]
        {
            (js_sys::Date::now() * 1000.0) as i64
        }
    }
}

/// The wall clock as a protobuf timestamp.
#[must_use]
pub fn now_ts() -> prost_types::Timestamp {
    ts(now_micros())
}

/// Microseconds since the Unix epoch as a protobuf timestamp.
#[must_use]
pub fn ts(micros: i64) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: micros.div_euclid(1_000_000),
        nanos: i32::try_from(micros.rem_euclid(1_000_000) * 1000).unwrap_or(0),
    }
}

/// The wall clock as nanoseconds, the roster verifier's unit.
#[must_use]
pub fn now_nanos() -> i128 {
    i128::from(now_micros()) * 1000
}

/// Sleeps for `duration` without blocking the executor.
pub async fn sleep(duration: Duration) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::time::sleep(duration).await;
    }
    #[cfg(target_arch = "wasm32")]
    {
        gloo_timers::future::sleep(duration).await;
    }
}
