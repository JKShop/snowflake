#![feature(slice_partition_dedup)]

use core::fmt;
use once_cell::sync::{Lazy, OnceCell};
use std::fmt::{Debug, Display, Formatter};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type TS = u128;
type Worker = u16;
type Sequence = u16;

static PREV_TS: Lazy<Mutex<TS>> = Lazy::new(|| Mutex::new(0));
static WORKER_ID: OnceCell<Worker> = OnceCell::new();
static SEQUENCE_ID: Lazy<Mutex<Sequence>> = Lazy::new(|| Mutex::new(0));

#[derive(Eq, PartialEq)]
pub struct Snowflake {
    pub timestamp: TS,
    pub worker_id: Worker,
    pub sequence_id: Sequence,
}

impl Snowflake {
    fn as_hex_string(&self) -> String {
        [
            format!("{:01$x}", self.timestamp, 16),
            format!("{:01$x}", self.worker_id, 4),
            format!("{:01$x}", self.sequence_id, 4),
        ]
        .join("")
    }
}

impl Display for Snowflake {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.as_hex_string())
    }
}

impl Debug for Snowflake {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.as_hex_string())
    }
}

/// Returns snowflake to use for db entry
/// # Arguments
/// # Returns
/// * String - Snowflake
impl Snowflake {
    pub async fn new() -> Self {
        // Don't change this order !
        let mut sequence_id_lock = SEQUENCE_ID.lock().unwrap();
        if *sequence_id_lock == u16::MAX {
            thread::sleep(Duration::from_nanos(10));
        }
        let mut prev_ts_lock = PREV_TS.lock().unwrap();

        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");
        let current_time = since_the_epoch.as_nanos();

        if *prev_ts_lock != current_time {
            *prev_ts_lock = current_time;
        } else {
            *sequence_id_lock += 1;
        }

        Snowflake {
            timestamp: current_time,
            worker_id: get_worker_id(),
            sequence_id: *sequence_id_lock,
        }
    }
}

/// Returns worker id
/// # Arguments
/// # Returns
/// * u32 - worker id
#[inline(always)]
pub fn get_worker_id() -> u16 {
    *WORKER_ID.get_or_init(|| {
        //TODO add actual worker id
        1
    })
}

#[cfg(test)]
mod tests {
    use crate::Snowflake;

    #[tokio::test]
    pub async fn test_a() {
        let mut timestamps = Vec::with_capacity(u16::MAX as usize);
        println!("Generating snowflakes");
        for _ in 0..(u16::MAX as usize * 4) {
            timestamps.push(Snowflake::new().await);
        }
        println!("Sorting snowflakes");
        timestamps.sort_unstable_by_key(|e| e.as_hex_string());
        println!("Deduping snowflakes");
        let (_, dups) = timestamps.partition_dedup_by_key(|e| e.as_hex_string());
        assert_eq!(dups.len(), 0);
    }
}
