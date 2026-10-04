pub mod board;
pub mod settle;
pub mod task;

pub use board::Board;
pub use settle::settle;
pub use task::{Bucket, Task, TaskState};

#[cfg(test)]
pub(crate) mod test_time {
    use chrono::{DateTime, Local, TimeZone};

    /// A deterministic local wall-clock instant, independent of the machine's
    /// timezone. `chrono::Local` is used because that is what the domain stores.
    pub fn at(year: i32, month: u32, day: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(year, month, day, 9, 0, 0)
            .single()
            .expect("09:00 always exists in a local timezone")
    }
}
