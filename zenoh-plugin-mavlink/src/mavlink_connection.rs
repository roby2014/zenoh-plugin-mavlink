use std::str::FromStr;

use mavlink::{
    async_peek_reader::AsyncPeekReader, common::MavMessage, MAVLinkV2MessageRaw, MavlinkVersion,
};
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize,
};
use tokio::{
    select,
    sync::broadcast::{Receiver, Sender},
};
use tracing::{debug, error, info, instrument};

use crate::protocol::Protocol;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct MAVLinkConnection {
    /// MAVLink endpoint, following [`mavlink::connect_async`] protocols.
    #[serde(default)]
    pub endpoint: String,
    /// MAVLink protocol version.
    #[serde(
        default = "default_mavlink_version",
        // FIXME: mavlink supports serde in their types, this shouldnt be required
        deserialize_with = "deserialize_version"
    )]
    pub mavlink_version: MavlinkVersion,
}

impl MAVLinkConnection {
    /// Handle a MAVLink connection.
    ///
    /// This means:
    /// - Read from the connection and broadcast outgoing MAVLink data.
    /// - Fetch broadcast channel and write incoming MAVLink data to the connection.
    #[instrument(skip(broadcast_channel))]
    pub async fn handle(
        self,
        mut broadcast_channel: (Sender<Protocol>, Receiver<Protocol>),
    ) -> std::io::Result<()> {
        info!("connecting");

        let mut connection = mavlink::connect_async::<MavMessage>(&self.endpoint).await?;
        connection.set_protocol_version(self.mavlink_version);

        info!("connected");

        loop {
            select! {
                // Read from the connection and broadcast outgoing MAVLink data.
                res = connection.recv() => {
                    match res {
                        Ok((header, msg)) => {
                            debug!(?header, ?msg, "received message from connection");

                            let mut raw = MAVLinkV2MessageRaw::new();
                            raw.serialize_message(header, &msg);
                            let broadcast_msg = Protocol::new(&self.endpoint, raw);

                            if let Err(e) = broadcast_channel.0.send(broadcast_msg) {
                                error!("could not send broadcast message: {e}");
                            } else {
                                debug!("forwarded raw mavlink message from connection to broadcast channel");

                            }
                        }
                        Err(e) => {
                            error!("failed to read from mavlink connection: {e}");
                            break;
                        }
                        // TODO: handle certain errors accordingly to decide if exit or try again (?)
                    }
                }
                // Fetch broadcast channel and write incoming MAVLink data to the connection.
                res = broadcast_channel.1.recv() => {
                    match res {
                        Ok(msg) => {
                            debug!("received message (id: {}) from broadcast channel (origin: {})", msg.raw_message.message_id(), msg.origin);
                            let mut bytes = AsyncPeekReader::new(msg.raw_message.raw_bytes());
                            let (header, message): (mavlink::MavHeader, mavlink::common::MavMessage) = mavlink::read_v2_msg_async(&mut bytes).await.unwrap();

                            if let Err(e) = connection.send(&header, &message).await {
                                error!("failed to write to mavlink connection {}: {:?}", self.endpoint, e);
                            } else {
                                debug!("forwarded message from broadcast channel to mavlink connection");
                            }
                        }
                        Err(e) => {
                            error!("failed to read from broadcast channel: {e}");
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

struct MAVLinkVersionVisitor;

fn default_mavlink_version() -> MavlinkVersion {
    MavlinkVersion::V2
}

fn deserialize_version<'de, D>(deserializer: D) -> Result<MavlinkVersion, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(MAVLinkVersionVisitor)
}

impl<'de> Visitor<'de> for MAVLinkVersionVisitor {
    type Value = MavlinkVersion;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str(r#"a integer representing the FCS MAVLink endpoint version"#)
    }

    // for `null` value
    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Err(de::Error::custom("Unspecified MAVLink version".to_string()))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match v {
            1 => Ok(MavlinkVersion::V1),
            2 => Ok(MavlinkVersion::V2),
            _ => Err(de::Error::custom("Invalid MAVLink version".to_string())),
        }
    }
}
