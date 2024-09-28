//! Inner messaging protocol for MAVLink connections and broadcast channels.

use mavio::{MavFrame};

#[derive(Clone, Debug)]
pub struct Protocol {
    /// Source of the message (e.g connection endpoint such as `serial:/dev/USB0:115200` or some identifier like `zenoh`).
    pub origin: String,
    pub mav_frame: MavFrame,
    pub timestamp: u64,
}

impl Protocol {
    pub fn new(origin: &str, mav_frame: MavFrame) -> Self {

        Self {
            origin: origin.to_string(),
            timestamp: chrono::Utc::now().timestamp_micros() as u64,
            mav_frame,
        }
    }

    pub fn new_with_timestamp(timestamp: u64, origin: &str, mav_frame: MavFrame) -> Self {
        Self {
            origin: origin.to_string(),
            timestamp,
            mav_frame,
        }
    }
}
