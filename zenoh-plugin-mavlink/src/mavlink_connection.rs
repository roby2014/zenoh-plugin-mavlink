use mavio::{io::connect_async, prelude::Versionless};
use serde::{
    Deserialize, Serialize,
};
use tokio::{
    select,
    sync::broadcast::{Receiver, Sender},
};
use tracing::{debug, error, info, instrument, trace};

use crate::protocol::Protocol;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct MAVLinkConnection {
    /// MAVLink endpoint, following [`mavlink::connect_async`] protocols.
    #[serde(default)]
    pub endpoint: String,
    // TODO: mav version ??? even tho Versionless does the job
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
        let mut connection = connect_async::<Versionless>(&self.endpoint).await?;
        info!("connected");

        loop {
            select! {
                // Read from the connection and broadcast outgoing MAVLink data.
                res = connection.recv() => {
                    match res {
                        Ok(frame) => {
                            debug!("received mav frame from connection (id = {})", frame.message_id());
                            trace!(?frame);
                            let broadcast_msg = Protocol::new(&self.endpoint, frame.into_mav_frame());
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
                            trace!("received message from broadcast channel");
                            // we only consume and write if its not the message we emitted
                            if msg.origin == self.endpoint { // FIXME: is this slow
                                trace!("ignoring messsage because it was produced by the same origin");
                                continue;
                            }

                            debug!("received message from broadcast channel (id: {}) (origin: {})", msg.mav_frame.message_id(), msg.origin);
                            if let Err(e) = connection.send(&msg.mav_frame.into_versionless()).await {
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
