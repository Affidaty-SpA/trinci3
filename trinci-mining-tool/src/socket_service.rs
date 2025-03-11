use crate::models::{BlockEvt, SocketCmd, TcpMsg};

use anyhow::{bail, Result};
use async_std::net::TcpStream;
use log::{debug, info};
use tokio::{
    select,
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
};
use tokio_util::sync::CancellationToken;
use trinci_core_new::{
    log_error,
    utils::{rmp_deserialize, rmp_serialize},
};
use trinci_node::services::{
    event_service::{Event, EventTopics},
    socket_service::{read_datagram, write_datagram, SocketRequest},
};

pub(crate) struct SocketService {
    pub block_event_channel: UnboundedSender<BlockEvt>,
    pub request_response_channel: Option<UnboundedSender<TcpMsg>>,
    pub stream: TcpStream,
}

impl SocketService {
    pub(crate) async fn start(
        block_event_channel: UnboundedSender<BlockEvt>,
        cancellation_token: CancellationToken,
        cmd_receiver: UnboundedReceiver<(SocketCmd, UnboundedSender<TcpMsg>)>,
        node_id: String,
        mut stream: TcpStream,
    ) -> Result<()> {
        // Subscribe tool to node's block events.
        let subscription_request = SocketRequest::Subscribe {
            events: EventTopics::BLOCK_EXEC,
            id: "MT".to_string(),
        };
        write_datagram(
            &mut stream,
            rmp_serialize(&subscription_request).map_err(|e| log_error!(e))?,
        )
        .await
        .map_err(|e| log_error!(e))?;

        if !rmp_deserialize::<bool>(
            &read_datagram(&mut stream)
                .await
                .map_err(|e| log_error!(e))?,
        )
        .map_err(|e| log_error!(e))?
        {
            bail!(log_error!(
                "Node {node_id}: problems during first BLOCK_EXEC event subscription."
            ));
        }
        info!("Node {node_id}: mining tool subscribed to the `BLOCK_EXEC` topic via socket interface.");

        let mut service = Self {
            stream,
            request_response_channel: None,
            block_event_channel,
        };

        info!("Node {node_id}: SocketService running.");

        service.run(cancellation_token, cmd_receiver).await
    }

    async fn run(
        &mut self,
        cancellation_token: CancellationToken,
        mut cmd_receiver: UnboundedReceiver<(SocketCmd, UnboundedSender<TcpMsg>)>,
    ) -> Result<()> {
        let reader = &mut self.stream.clone();
        loop {
            select!(
                _ = cancellation_token.cancelled() => {return Ok(())}
                buf = read_datagram(reader) => {
                    if let Ok(buf) = buf {
                        match rmp_deserialize::<TcpMsg>(&buf).map_err(|e| {cancellation_token.cancel(); log_error!(e)})? {
                            TcpMsg::BlockEvt(evt) => {
                                match evt {
                                    Event::BlockEvent { block, txs, origin } => self.block_event_channel.send(BlockEvt { block, txs_hashes: txs, origin})
                                    .map_err(|e| {cancellation_token.cancel(); log_error!(e)})?
                                    ,
                                    _ => {}
                                }
                            }
                            TcpMsg::ResponseTopics(outcome) => {
                                if let Some(res_chan) = &self.request_response_channel {
                                    // Error logged only because it can simply be originated by a timeout.
                                    let _res = res_chan
                                        .send(TcpMsg::ResponseTopics(outcome))
                                        .map_err(|e| log_error!(e));
                                }
                            }
                            TcpMsg::ResponseTxSign(response) => {
                                if let Some(res_chan) = &self.request_response_channel {
                                    // Error logged only because it can simply be originated by a timeout.
                                    let _res = res_chan
                                        .send(TcpMsg::ResponseTxSign(response))
                                        .map_err(|e| log_error!(e));
                                }
                            }
                        };
                    }
                }
                cmd = cmd_receiver.recv() => {
                    if let Some((cmd, response_chan)) = cmd {
                        self.handle_cmd(cmd, response_chan).await.map_err(|e| {cancellation_token.cancel(); e})?;
                    }
                }
            );
        }
    }

    async fn handle_cmd(
        &mut self,
        cmd: SocketCmd,
        response_chan: UnboundedSender<TcpMsg>,
    ) -> Result<()> {
        debug!("Handling socket command: {cmd:?}.");
        match cmd {
            SocketCmd::Subscribe(event_topics) => {
                // Subscribe tool to node's block events.
                let subscription_request = SocketRequest::Subscribe {
                    events: event_topics,
                    id: "MT".to_string(),
                };

                write_datagram(
                    &mut self.stream,
                    rmp_serialize(&subscription_request).map_err(|e| log_error!(e))?,
                )
                .await
                .map_err(|e| log_error!(e))?;
            }
            SocketCmd::Unsubscribe(event_topics) => {
                // Subscribe tool to node's block events.
                let subscription_request = SocketRequest::Unsubscribe {
                    events: event_topics,
                    id: "MT".to_string(),
                };

                write_datagram(
                    &mut self.stream,
                    rmp_serialize(&subscription_request).map_err(|e| log_error!(e))?,
                )
                .await
                .map_err(|e| log_error!(e))?;
            }
            SocketCmd::SubmitTx(request) => {
                if let SocketRequest::SubmitTx { .. } = request {
                    write_datagram(
                        &mut self.stream,
                        rmp_serialize(&request).map_err(|e| log_error!(e))?,
                    )
                    .await
                    .map_err(|e| log_error!(e))?;
                }
            }
        }

        self.request_response_channel = Some(response_chan);

        Ok(())
    }
}
