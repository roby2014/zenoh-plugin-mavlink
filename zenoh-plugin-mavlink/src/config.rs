use serde::Deserialize;

use crate::mavlink_connection::MAVLinkConnection;

pub const DEFAULT_NODENAME: &str = "zenoh_bridge_mavlink";
pub const DEFAULT_WORK_THREAD_NUM: usize = 2;
pub const DEFAULT_MAX_BLOCK_THREAD_NUM: usize = 50;
pub const DEFAULT_BROADCAST_CHANNEL_CAPACITY: usize = 1024;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub mavlink_connections: Vec<MAVLinkConnection>,
    #[serde(default = "broadcast_channel_capacity")]
    pub broadcast_channel_capacity: usize,
    #[serde(default)]
    pub to_zenoh: bool,
    #[serde(default)]
    pub from_zenoh: bool,
    #[serde(default = "default_work_thread_num")]
    pub work_thread_num: usize,
    #[serde(default = "default_max_block_thread_num")]
    pub max_block_thread_num: usize,
}

fn broadcast_channel_capacity() -> usize {
    DEFAULT_BROADCAST_CHANNEL_CAPACITY
}

fn default_work_thread_num() -> usize {
    DEFAULT_WORK_THREAD_NUM
}

fn default_max_block_thread_num() -> usize {
    DEFAULT_MAX_BLOCK_THREAD_NUM
}
