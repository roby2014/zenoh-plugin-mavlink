use serde::Deserialize;
use zenoh::prelude::OwnedKeyExpr;

use crate::mavlink_connection::MAVLinkConnection;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub mavlink_connections: Vec<MAVLinkConnection>,
    #[serde(default)]
    pub broadcast_channel_capacity: usize,
    #[serde(default)]
    pub group_member_id: Option<OwnedKeyExpr>,
}
