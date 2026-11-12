use std::fmt::Debug;
use std::future::Future;
use std::time::Duration;

const FIRST_DELAY_SECS: u64 = 5;
const MAX_DELAY_SECS: u64 = 300;

pub fn backoff_delay(attempt: u32) -> Duration {
    let secs = 2u64
        .checked_pow(attempt)
        .and_then(|m| m.checked_mul(FIRST_DELAY_SECS))
        .map_or(MAX_DELAY_SECS, |s| s.min(MAX_DELAY_SECS));
    Duration::from_secs(secs)
}

pub async fn retry_until_ok<T, E, F, Fut>(what: &str, mut op: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: Debug,
{
    let mut attempt = 0;
    loop {
        match op().await {
            Ok(value) => return value,
            Err(e) => {
                let delay = backoff_delay(attempt);
                tracing::warn!(what, attempt, delay_secs = delay.as_secs(), error = ?e, "Failed, retrying");
                tokio::time::sleep(delay).await;
                attempt = attempt.saturating_add(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use tokio::time::Instant;

    /// 1回目の待ち時間は5秒であること
    #[test]
    fn test_first_delay_is_5_seconds() {
        assert_eq!(backoff_delay(0), Duration::from_secs(5));
    }

    /// 失敗するたびに待ち時間が倍になること
    #[test]
    fn test_delay_doubles() {
        assert_eq!(backoff_delay(1), Duration::from_secs(10));
        assert_eq!(backoff_delay(2), Duration::from_secs(20));
    }

    /// 待ち時間は300秒より長くならないこと
    #[test]
    fn test_delay_is_capped_at_300_seconds() {
        assert_eq!(backoff_delay(10), Duration::from_secs(300));
        assert_eq!(backoff_delay(u32::MAX), Duration::from_secs(300));
    }

    /// 1回目で成功したら、待たずにその値を返すこと
    #[tokio::test(start_paused = true)]
    async fn test_returns_immediately_on_first_success() {
        let start = Instant::now();

        let value = retry_until_ok("test", || async { Ok::<_, ()>(42) }).await;

        assert_eq!(value, 42);
        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    /// 失敗が続いても、成功するまでやり直してその値を返すこと
    #[tokio::test(start_paused = true)]
    async fn test_retries_until_success() {
        let calls = Cell::new(0);

        let value = retry_until_ok("test", || {
            calls.set(calls.get() + 1);
            let n = calls.get();
            async move { if n < 4 { Err("down") } else { Ok(n) } }
        })
        .await;

        assert_eq!(value, 4);
        assert_eq!(calls.get(), 4);
    }

    /// やり直すまでに、失敗した回数に応じた時間だけ待つこと
    #[tokio::test(start_paused = true)]
    async fn test_waits_backoff_between_attempts() {
        let calls = Cell::new(0);
        let start = Instant::now();

        retry_until_ok("test", || {
            calls.set(calls.get() + 1);
            let n = calls.get();
            async move { if n < 4 { Err("down") } else { Ok(()) } }
        })
        .await;

        assert_eq!(start.elapsed(), Duration::from_secs(5 + 10 + 20));
    }
}
