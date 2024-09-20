//! Inner messaging protocol for MAVLink connections and broadcast channels.

use mavlink::MAVLinkV2MessageRaw;

#[derive(Clone, Debug, PartialEq)]
pub struct Protocol {
    /// Source of the message (e.g connection endpoint such as `serial:/dev/USB0:115200` or some identifier like `zenoh`).
    pub origin: String,
    /// MAVLink raw message.
    pub raw_message: MAVLinkV2MessageRaw,
    pub timestamp: u64,
}

impl Protocol {
    pub fn new(origin: &str, message: MAVLinkV2MessageRaw) -> Self {
        Self {
            origin: origin.to_string(),
            timestamp: chrono::Utc::now().timestamp_micros() as u64,
            raw_message: message,
        }
    }

    pub fn new_with_timestamp(timestamp: u64, origin: &str, message: MAVLinkV2MessageRaw) -> Self {
        Self {
            origin: origin.to_string(),
            timestamp,
            raw_message: message,
        }
    }
}
