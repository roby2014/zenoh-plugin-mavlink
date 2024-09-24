use serde::Deserialize;
use zenoh::key_expr::OwnedKeyExpr;

use crate::mavlink_connection::MAVLinkConnection;

pub const DEFAULT_NODENAME: &str = "zenoh_bridge_mavlink";
pub const DEFAULT_WORK_THREAD_NUM: usize = 2;
pub const DEFAULT_MAX_BLOCK_THREAD_NUM: usize = 50;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub mavlink_connections: Vec<MAVLinkConnection>,
    #[serde(default)]
    pub broadcast_channel_capacity: usize,
    #[serde(default = "default_work_thread_num")]
    pub work_thread_num: usize,
    #[serde(default = "default_max_block_thread_num")]
    pub max_block_thread_num: usize,
}

fn default_work_thread_num() -> usize {
    DEFAULT_WORK_THREAD_NUM
}

fn default_max_block_thread_num() -> usize {
    DEFAULT_MAX_BLOCK_THREAD_NUM
}
