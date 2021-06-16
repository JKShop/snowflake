#![feature(slice_partition_dedup)]
#![forbid(unsafe_code)]
#![deny(clippy::needless_borrow, clippy::unwrap_used)]
#![deny(unused_imports)]
#![forbid(missing_docs)]
#![feature(async_closure)]
//! This crate generates Snowflake id's
//! It get's it's worker id from an remote endpoint and re-verifies automatically

use core::fmt;
use once_cell::sync::{Lazy, OnceCell};
use reqwest::StatusCode;
use serde::Deserialize;
use std::fmt::{Debug, Display, Formatter};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{env, thread};

/// Holds response for / request
#[derive(Deserialize, Debug)]
struct CoordinatorResponse {
    /// Worker id of requester
    pub id: WorkerId,
    /// Request timestamp
    pub ts: CoordinatorTimestamp,
    /// Last accepted timestamp, before id is given out again
    pub re_ts: CoordinatorTimestamp,
}
type CoordinatorTimestamp = u64;
type NanoTimestamp = u128;
type WorkerId = u16;
type UsageId = u8;
type SequenceId = u8;

const PRE_TIME: u64 = 300;

static PREV_TS: Lazy<Mutex<NanoTimestamp>> = Lazy::new(|| Mutex::new(0));
static WORKER_ID: OnceCell<WorkerId> = OnceCell::new();
static SEQUENCE_ID: Lazy<Mutex<SequenceId>> = Lazy::new(|| Mutex::new(0));

/// Holds an snowflake id
#[derive(Eq, PartialEq)]
pub struct Snowflake {
    /// Snowflake generation timestamp
    pub timestamp: NanoTimestamp,
    /// Snowflake worker id
    pub worker_id: WorkerId,
    /// Snowflake sequence id (if multiple where to be generated in the same nano sec)
    pub sequence_id: SequenceId,
    /// User defined usage id (can for example define regions)
    pub usage_id: UsageId,
}

impl Snowflake {
    fn as_hex_string(&self) -> String {
        let x: String = [
            format!("{:01$x}", self.timestamp, 16),
            format!("{:01$x}", self.worker_id, 4),
            format!("{:01$x}", self.sequence_id, 2),
            format!("{:01$x}", self.usage_id, 2),
        ]
        .join("");
        x.trim().to_string()
    }
}

impl Display for Snowflake {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_hex_string())
    }
}

impl Debug for Snowflake {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_hex_string())
    }
}

/// Returns snowflake to use for db entry
/// # Arguments
/// # Returns
/// * String - Snowflake
impl Snowflake {
    /// Generates a new snowflake
    pub async fn new(usage_id: UsageId) -> Self {
        // Don't change this order !
        let mut sequence_id_lock = SEQUENCE_ID.lock().expect("Couldn't lock SEQUENCE_ID mutex");
        if *sequence_id_lock == SequenceId::MAX {
            thread::sleep(Duration::from_nanos(10));
        }
        let mut prev_ts_lock = PREV_TS.lock().expect("Couldn't lock PREV_TS mutex");

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
            worker_id: get_worker_id().await,
            sequence_id: *sequence_id_lock,
            usage_id
        }
    }
}

/// Returns worker id
/// # Arguments
/// # Returns
/// * u32 - worker id
#[inline(always)]
async fn get_worker_id() -> WorkerId {
    match WORKER_ID.get(){
        None => {
            let id = init_worker_id().await;
            WORKER_ID.set(id).expect("WORKER_ID is set but unset ?");
            id
        }
        Some(v) => {*v}
    }
}

async fn init_worker_id() -> WorkerId {
    let coordinator_url = env::var("SNOWFLAKE.COORDINATOR").expect("Coordinator url not set");
    log::debug!("Coordinator url: {}", coordinator_url);
    let response =
        reqwest::blocking::get(&coordinator_url).expect("Failed to get Coordinator response");
    if response.status() != StatusCode::OK {
        panic!("Coordinator gave non-200 response !\n{:?}", response);
    } else {
        let cr: CoordinatorResponse =
            serde_json::from_str(&response.text().expect("Couldn't parse response as text"))
                .expect("Couldn't parse coordinator response !");

        let local_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();

        if (local_ts as i128 - cr.ts as i128).abs() > PRE_TIME as i128 {
            log::error!("Local TS: {}", local_ts);
            log::error!("Rev TS: {}", cr.ts);
            log::error!("Diff: {}", (local_ts as i128 - cr.ts as i128).abs());
            panic!(
                "Coordinator time and local time since unix epoch differ by more then {} seconds !",PRE_TIME
            )
        }

        if cr.re_ts < local_ts  {
            panic!("Coordinator re-verify time is smaller then local time")
        }

        let time_to_next_sleep =
            cr.re_ts - PRE_TIME /* Attempts to verify PRE_TIME secs before it has to be done */ - local_ts;
        let id = cr.id;
        let curl = coordinator_url;

        thread::spawn(move || {
            sleep(Duration::from_secs(time_to_next_sleep));
            log::info!("re-verifying snowflake worker id");
            loop {
                let mut verify_response =
                    reqwest::blocking::get(format!("{}/reverify/{}", curl, id));
                let mut re_verify = 0;
                while verify_response.is_err() {
                    if re_verify >= 10 {
                        panic!("Failed to re-verify snowflake worker id !")
                    }
                    verify_response = reqwest::blocking::get(format!("{}/reverify/{}", curl, id));
                    log::warn!("re-verifying failed. Attempt: {}", re_verify);
                    re_verify += 1;
                    sleep(Duration::from_secs(1));
                }

                match verify_response {
                    Ok(v) => {
                        let body = v
                            .text()
                            .expect("Couldn't read body from re-verify response");
                        let rev: CoordinatorResponse = serde_json::from_str(&body)
                            .expect("Couldn't deserialize re-verify response");

                        if rev.id != id {
                            panic!("Snowflake worker id changed ! {} -> {}", rev.id, id);
                        }

                        let local_ts = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .expect("Time went backwards")
                            .as_secs();

                        if (local_ts as i128 - rev.ts as i128).abs() > PRE_TIME as i128{
                            log::error!("Local TS: {}", local_ts);
                            log::error!("Rev TS: {}", rev.ts);
                            log::error!("Diff: {}", (local_ts as i128 - rev.ts as i128).abs());
                            panic!(
                                "Coordinator time and local time since unix epoch differ by more then {} seconds !",PRE_TIME
                            )
                        }
                        log::info!("Snowflake re-validated, next: {}", time_to_next_sleep);

                        sleep(Duration::from_secs(time_to_next_sleep))
                    }
                    Err(_) => {
                        unreachable!("re_verify should panic before coming here !")
                    }
                }
            }
        });

        id
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use crate::{Snowflake, UsageId};

    #[tokio::test]
    pub async fn test_a() {
        let mut timestamps = Vec::with_capacity(u16::MAX as usize);
        println!("Generating snowflakes");
        for i in 0..(u16::MAX as usize * 4) {
            timestamps.push(Snowflake::new((i % (UsageId::MAX as usize)) as u8).await);
        }
        println!("Sorting snowflakes");
        timestamps.sort_unstable_by_key(|e| e.as_hex_string());
        println!("Deduping snowflakes");
        let (_, dups) = timestamps.partition_dedup_by_key(|e| e.as_hex_string());
        assert_eq!(dups.len(), 0);
    }

    #[tokio::test]
    pub async fn test_b() {
        let snowflake = Snowflake::new(0).await;
        println!("{:?}", snowflake);
    }
}
