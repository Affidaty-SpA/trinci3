//! The `socket_service` module implements ta socket interface used by external clients to subtribe to the nodes events.
//! In case the `mining` feature is enabled, the interface is used to interact with the TRINCI mining tool, that will send
//! `SignAndSubmitTx` messages that the socket service will get, check if the signer of the message is the expected mining tool
//! and in positive case will sign the transaction and submit it to the network. So in the case of `mining` feature enabled
//! is highly encouraged to do not expose the socket interface to the extern.

use crate::{
    artifacts::{
        errors::{CommError, NodeError},
        messages::{send_req_async, Comm, CommKind, Req, Res},
        models::{Block, Confirmation, Transaction},
    },
    services::event_service::EventTopics,
    Services,
};

use trinci_core_new::{
    artifacts::{
        messages::send_req,
        models::{Services as CoreServices, Transactable},
    },
    crypto::{
        hash::Hash,
        identity::{TrinciKeyPair, TrinciPublicKey},
    },
    log_error,
    utils::{rmp_deserialize, rmp_serialize},
};

#[cfg(feature = "mining")]
use crate::artifacts::{
    messages::{send_msg_async, Msg},
    models::{SignedTransaction, TransactionData, TransactionDataV1},
};

#[cfg(feature = "mining")]
use trinci_core_new::artifacts::{
    messages::{send_msg, Comm as CoreComm, CommKind as CoreCommKind, Msg as CoreMsg},
    models::Hashable,
};

#[cfg(feature = "mining")]
use crate::consts::MT_PK_FILEPATH;

use async_std::{
    channel::{unbounded as async_unbounded, Sender as AsyncSender},
    io,
    net::{TcpListener, TcpStream},
};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use futures::{
    future::{select, Either},
    AsyncReadExt, AsyncWriteExt, StreamExt,
};
use log::{debug, info, trace, warn};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::Read,
    sync::{Arc, RwLock},
};

type MiningToolKey = Arc<RwLock<Option<TrinciPublicKey>>>;

pub struct SocketService {
    pub node_id: String,
    pub node_info: NodeInfo,
    pub core_services: CoreServices<Hash, Block, Confirmation, Transaction>,
    pub services: Services,
    #[cfg(feature = "mining")]
    mining_tool_pub_key: MiningToolKey,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NodeInfo {
    /// Listening IP address (e.g.: "127.0.0.1" for localhost)
    pub addr: String,
    pub mining_account_id: Option<String>,
    pub network: String,
    pub nonce: [u8; 8],
    /// Node public key.
    pub public_key: TrinciPublicKey,
    /// Listening REST port.
    pub rest_port: u16,
}

// TODO: it would be interesting to find a way, maybe with some serializer decorator
//       to omit the id.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum SocketRequest {
    #[serde(rename = "1")]
    Subscribe {
        id: String,
        /// Events set (bitflags).
        events: EventTopics,
    },
    /// Unsubscribe from a set of blockchain events.
    #[serde(rename = "2")]
    Unsubscribe {
        id: String,
        /// Events set (bitflags).
        events: EventTopics,
    },
    #[cfg(feature = "mining")]
    #[serde(rename = "3")]
    SubmitTx {
        tx_data: TransactionDataV1,
        signature: Vec<u8>,
    },
}

impl Serialize for EventTopics {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(self.bits())
    }
}

impl<'de> Deserialize<'de> for EventTopics {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct EventVisitor;

        impl<'de> serde::de::Visitor<'de> for EventVisitor {
            type Value = u8;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("u8")
            }

            fn visit_u8<R>(self, value: u8) -> std::result::Result<u8, R> {
                Ok(value)
            }
        }

        let bits = deserializer.deserialize_u8(EventVisitor)?;
        let event = EventTopics::from_bits(bits).ok_or(..).unwrap();
        Ok(event)
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct SocketResponse {
    /// Serialized message bytes.
    #[serde(with = "serde_bytes")]
    buf: Vec<u8>,
}

impl SocketService {
    pub fn new(
        node_id: &str,
        core_services: CoreServices<Hash, Block, Confirmation, Transaction>,
        services: Services,
        node_info: NodeInfo,
    ) -> Self {
        #[cfg(feature = "mining")]
        let mining_tool_pub_key = match File::open(MT_PK_FILEPATH) {
            Ok(mut file) => {
                let mut buf = Vec::new();
                let _res = file
                    .read_to_end(&mut buf)
                    .map_err(|e| log_error!(e))
                    .unwrap();
                Arc::new(RwLock::new(Some(
                    rmp_deserialize::<TrinciPublicKey>(&buf)
                        .map_err(|e| log_error!(e))
                        .unwrap(),
                )))
            }
            Err(_) => Arc::new(RwLock::new(None)),
        };

        Self {
            node_id: node_id.into(),
            node_info,
            core_services,
            services,
            #[cfg(feature = "mining")]
            mining_tool_pub_key,
        }
    }

    pub async fn start(
        addr: String,
        keypair: Option<Arc<TrinciKeyPair>>,
        node_id: &str,
        node_info: NodeInfo,
        core_services: CoreServices<Hash, Block, Confirmation, Transaction>,
        services: Services,
        tcp_port: u16,
    ) {
        let mut service = Self::new(node_id, core_services, services, node_info);

        info!("SocketService successfully started.");
        service.run(keypair, addr, tcp_port).await
    }

    #[allow(unused_variables)]
    async fn run(&mut self, keypair: Option<Arc<TrinciKeyPair>>, addr: String, tcp_port: u16) {
        let signer_service_sender = {
            #[cfg(feature = "mining")]
            {
                let (signer_service_sender, signer_service_receiver): (
                    Sender<Comm>,
                    Receiver<Comm>,
                ) = unbounded();
                let node_id = self.node_id.clone();
                let p2p_service_sender = self.services.p2p_service.clone();
                let transaction_service_sender = self.core_services.transaction_service.clone();
                let mining_tool_pub_key = self.mining_tool_pub_key.clone();

                std::thread::spawn(move || {
                    while let Ok(comm) = signer_service_receiver.recv() {
                        if let CommKind::Msg(Msg::SignAndSubmitTx {
                            data,
                            res_chan,
                            signature,
                        }) = comm.kind
                        {
                            let Ok(pub_key) =
                                Self::get_mining_tool_pub_key(mining_tool_pub_key.clone())
                            else {
                                continue;
                            };

                            let pub_key = mining_tool_pub_key.read().unwrap();
                            if !pub_key
                                .as_ref()
                                .unwrap()
                                .verify(&rmp_serialize(&data).unwrap(), &signature)
                            {
                                // In case sign do not math, just jump to the next request.
                                warn!(
                                    "`SubmitTx` request sign do not match the paired mining tool"
                                );
                                continue;
                            }

                            let tx_data = TransactionData::V1(data);

                            let Ok(signature) = keypair
                                .as_ref()
                                .expect(
                                    "Compiling with 'mining' feature a keypair is always present",
                                )
                                .sign(&rmp_serialize(&tx_data).unwrap())
                            else {
                                warn!(
                                    "Node {}, SocketService: failed to sign transaction data",
                                    node_id
                                );
                                continue;
                            };
                            let tx = Transaction::UnitTransaction(SignedTransaction {
                                data: tx_data,
                                signature,
                            });

                            let size = rmp_serialize(&tx)
                                .expect("Transaction should always be serializable")
                                .len();

                            let _res = res_chan.send(hex::encode(tx.get_hash()));

                            send_msg_async(
                                &node_id,
                                "SocketService",
                                "P2PService",
                                &p2p_service_sender,
                                Comm::new(CommKind::Msg(Msg::PropagateTxToPeers(tx.clone())), None),
                            );

                            send_msg(
                                &node_id,
                                "SocketService",
                                "TransactionService",
                                &transaction_service_sender,
                                CoreComm::new(
                                    CoreCommKind::Msg(CoreMsg::AddTx((tx, size, None))),
                                    None,
                                ),
                            );
                        }
                    }
                });
                Some(signer_service_sender)
            }
            #[cfg(not(feature = "mining"))]
            {
                None
            }
        };

        let listener = TcpListener::bind((addr.as_str(), tcp_port))
            .await
            .expect("Failed TcpListener binding"); // If the binding doesn't succeed, it has no reason to go forward.
        listener
            .incoming()
            .for_each_concurrent(1, |stream| {
                let services = self.services.clone();
                let node_id = self.node_id.clone();
                let node_info = self.node_info.clone();
                let signer_service_sender = signer_service_sender.clone();

                let fut = async move {
                    if let Ok(stream) = stream {
                        debug!("New socket connection established");
                        let _ = Self::connection_handler(
                            stream,
                            services,
                            &node_id,
                            node_info,
                            signer_service_sender,
                        )
                        .await;
                    } else {
                        debug!("Spurious socket connection");
                    }
                };
                async_std::task::spawn(fut)
            })
            .await;
    }

    #[cfg(feature = "mining")]
    fn get_mining_tool_pub_key(
        mining_tool_pub_key: MiningToolKey,
    ) -> Result<TrinciPublicKey, NodeError<Transaction>> {
        let pub_key_option = match mining_tool_pub_key.read() {
            Ok(pub_key_option) => pub_key_option,
            Err(e) => {
                log_error!(e);
                return Err(NodeError::GenericError);
            }
        };
        if let Some(pub_key) = &*pub_key_option {
            Ok(pub_key.clone())
        } else {
            drop(pub_key_option);

            let mut buf = Vec::new();
            let mut file = match File::open(MT_PK_FILEPATH) {
                Ok(file) => file,
                Err(e) => {
                    log_error!(e);
                    return Err(NodeError::GenericError);
                }
            };

            if let Err(e) = file.read_to_end(&mut buf) {
                log_error!(e);
                return Err(NodeError::GenericError);
            }
            let pub_key: TrinciPublicKey = rmp_deserialize(&buf).map_err(|e| log_error!(e))?;

            match mining_tool_pub_key.write() {
                Ok(mut pub_key_write_lock) => {
                    *pub_key_write_lock = Some(pub_key.clone());
                }
                Err(e) => {
                    log_error!(e);
                    return Err(NodeError::GenericError);
                }
            }

            Ok(pub_key)
        }
    }

    async fn connection_handler(
        mut reader: TcpStream,
        services: Services,
        node_id: &str,
        mut node_info: NodeInfo,
        signer_service_sender: Option<Sender<Comm>>,
    ) -> Result<(), NodeError<Comm>> {
        let mut id = None;
        let mut writer = reader.clone();
        let (event_sender, event_receiver) = async_unbounded();

        // To confirm connection establishment, node sends an ACK,
        // it contains some node infos useful to interact with it and know its identity.

        // Retrieve missing `node_info`
        let (res_sender, res_receiver) = bounded(2);
        if let Ok(Res::GetMiningAccount(mining_account)) = send_req::<Comm, Res, CommError<Comm>>(
            node_id,
            "SocketService",
            "DBService",
            &services.db_service,
            Comm::new(CommKind::Req(Req::GetMiningAccount), Some(res_sender)),
            &res_receiver,
        ) {
            node_info.mining_account_id = mining_account;
        } else {
            warn!("Unexpected response from DBService during mining account retrieval.");
        }

        write_datagram(
            &mut writer,
            rmp_serialize(&node_info).map_err(|e| log_error!(e))?,
        )
        .await
        .map_err(|e| log_error!(e))?;

        loop {
            async_std::task::sleep(std::time::Duration::from_millis(500)).await;

            match select(Box::pin(read_datagram(&mut reader)), event_receiver.recv()).await {
                Either::Left((buf, _)) => {
                    if let Ok(buf) = buf {
                        let req =
                            rmp_deserialize::<SocketRequest>(&buf).map_err(|e| log_error!(e))?;

                        let res = Self::handle_socket_requests(
                            req,
                            &mut id,
                            node_id,
                            &services,
                            &event_sender,
                            &signer_service_sender,
                        )?;

                        write_datagram(&mut writer, res.buf)
                            .await
                            .map_err(|e| log_error!(e))?;
                    } else {
                        debug!("Read from buffer failed, client might connection might be dropped. Exiting the loop.");

                        // Removing client from EventService switchboard.
                        // If `id` is none, it means that client is not subscribed to any event.
                        if let Some(id) = id {
                            Self::handle_disconnected_client(node_id, id, &services.event_service)?
                        }

                        return Ok(());
                    }
                }
                Either::Right((event, _)) => {
                    debug!("Node {node_id} received event. Sending to connection {id:?}");

                    if let Ok(event) = event {
                        if let Err(e) = write_datagram(&mut writer, event).await {
                            if let Some(id) = id {
                                Self::handle_disconnected_client(
                                    node_id,
                                    id,
                                    &services.event_service,
                                )?
                            }
                            log_error!(e);
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    fn handle_disconnected_client(
        node_id: &str,
        id: u64,
        event_service: &AsyncSender<Comm>,
    ) -> Result<(), NodeError<Comm>> {
        let chan: (Sender<Res>, Receiver<Res>) = unbounded();
        match send_req_async(
            node_id,
            "SocketService",
            "EventService",
            event_service,
            Comm::new(CommKind::Req(Req::Disconnect(id)), Some(chan.0)),
            &chan.1,
        ) {
            Ok(Res::Disconnect(success)) => {
                if !success {
                    warn!(
                        "Unsubscribe failed, client not present in the EventService switchboard."
                    );
                    return Err(NodeError::GenericError);
                }

                Ok(())
            }
            Ok(_) => Err(log_error!(CommError::InvalidComm::<Comm>).into()),
            Err(e) => Err(log_error!(e).into()),
        }
    }

    fn handle_socket_requests(
        req: SocketRequest,
        id: &mut Option<u64>,
        node_id: &str,
        services: &Services,
        event_sender: &AsyncSender<Vec<u8>>,
        signer_service_sender: &Option<Sender<Comm>>,
    ) -> Result<SocketResponse, NodeError<Comm>> {
        match req {
            SocketRequest::Subscribe { id: _, events } => {
                trace!("Node {node_id} received `Subscribe` SocketRequest to topics {events:?} from connection {id:?}");

                let chan: (Sender<Res>, Receiver<Res>) = unbounded();

                let connection_id = match send_req_async(
                    node_id,
                    "SocketService",
                    "EventService",
                    &services.event_service,
                    Comm::new(
                        CommKind::Req(Req::Subscribe {
                            event_topics: events,
                            event_sender: event_sender.clone(),
                            subscriber_id: *id,
                        }),
                        Some(chan.0),
                    ),
                    &chan.1,
                ) {
                    Ok(Res::Subscribe(Some(connection_id))) => connection_id,
                    Ok(_) => return Err(log_error!(CommError::InvalidComm::<Comm>).into()),
                    Err(e) => return Err(log_error!(e).into()),
                };

                if id.is_none() {
                    *id = Some(connection_id);
                }

                trace!("`Subscribe` request for id {id:?} has been handled");
            }
            SocketRequest::Unsubscribe { id: _, events } => {
                trace!("Node {node_id} received `Unsubscribe` SocketRequest from topics {events:?} from connection {id:?}");

                let chan: (Sender<Res>, Receiver<Res>) = unbounded();

                if let Some(id) = id {
                    match send_req_async(
                        node_id,
                        "SocketService",
                        "EventService",
                        &services.event_service,
                        Comm::new(
                            CommKind::Req(Req::Unsubscribe {
                                event_topics: events,
                                subscriber_id: *id,
                            }),
                            Some(chan.0),
                        ),
                        &chan.1,
                    ) {
                        Ok(Res::Unsubscribe(success)) => {
                            if !success {
                                warn!("Unsubscribe failed");
                                return Err(NodeError::GenericError);
                            }
                        }
                        Ok(_) => return Err(log_error!(CommError::InvalidComm::<Comm>).into()),
                        Err(e) => return Err(log_error!(e).into()),
                    };
                }
            }
            #[cfg(feature = "mining")]
            SocketRequest::SubmitTx { tx_data, signature } => {
                trace!("Node {node_id} received `SubmitTx` SocketRequest for transaction {}, method {}, from connection {id:?}", hex::encode(tx_data.primary_hash()?), tx_data.method);

                let (res_chan, response_receiver) = bounded(2);
                if let Err(e) = signer_service_sender
                    .as_ref()
                    .expect("Compiling with 'mining' feature a keypair is always present")
                    .send(Comm::new(
                        CommKind::Msg(Msg::SignAndSubmitTx {
                            data: tx_data,
                            res_chan,
                            signature,
                        }),
                        None,
                    ))
                {
                    return Err(CommError::SendError::<Comm>(log_error!(e)).into());
                };

                return match response_receiver.recv() {
                    Ok(tx_hash) => Ok(SocketResponse {
                        buf: rmp_serialize(&tx_hash)?,
                    }),
                    Err(_e) => todo!(),
                };
            }
        };

        Ok(SocketResponse {
            buf: rmp_serialize(&true)?,
        })
    }
}

pub async fn read_datagram(stream: &mut TcpStream) -> Result<Vec<u8>, std::io::Error> {
    // Read the header
    let mut head = [0u8; 4];
    stream.read_exact(&mut head).await?;
    let len = u32::from_be_bytes(head);
    // Read the body
    let mut buf = vec![0u8; len as usize];
    stream.read_exact(&mut buf).await?;

    Ok(buf)
}

pub async fn write_datagram(stream: &mut TcpStream, buf: Vec<u8>) -> Result<(), io::Error> {
    // Use vectored IO to write both at once. It is fundamental to perform a single
    // write operation to avoid problems with read_datagram() future polling in the
    // socket_service main loop.
    let written = stream
        .write_vectored(&[
            io::IoSlice::new(&(buf.len() as u32).to_be_bytes()),
            io::IoSlice::new(&buf),
        ])
        .await?;

    if written != buf.len() {
        warn!(
            "The `write_datagram` function written {written} but was expected to write {} bytes",
            buf.len()
        )
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        artifacts::{
            messages::Comm,
            models::{Block, Confirmation, Transaction},
        },
        consts::NONCE,
        services::{
            event_service::{EventService, EventTopics},
            socket_service::{
                read_datagram, write_datagram, NodeInfo, SocketRequest, SocketService,
            },
        },
        Services,
    };

    use async_std::{
        channel::{unbounded as async_unbounded, Receiver as AsyncReceiver, Sender as AsyncSender},
        net::{Shutdown, TcpStream},
    };
    use crossbeam_channel::{unbounded, Receiver, Sender};
    use std::sync::Arc;
    use trinci_core_new::{
        artifacts::{
            messages::Comm as CoreComm,
            models::{CurveId, Services as CoreServices},
        },
        crypto::{
            hash::Hash,
            identity::{TrinciKeyPair, TrinciPublicKey},
        },
        utils::{rmp_deserialize, rmp_serialize},
    };

    #[derive(Clone)]
    #[allow(unused)]
    struct Receivers {
        db_service: Receiver<Comm>,
        event_service: AsyncReceiver<Comm>,
        p2p_service: AsyncReceiver<Comm>,
        wasm_service: Receiver<Comm>,
    }

    fn mock_core_services() -> CoreServices<Hash, Block, Confirmation, Transaction> {
        let comms_block_service: (
            Sender<CoreComm<Hash, Block, Confirmation, Transaction>>,
            Receiver<CoreComm<Hash, Block, Confirmation, Transaction>>,
        ) = unbounded();
        let comms_consensus_service: (
            Sender<CoreComm<Hash, Block, Confirmation, Transaction>>,
            Receiver<CoreComm<Hash, Block, Confirmation, Transaction>>,
        ) = unbounded();
        let comms_drand_service: (
            Sender<CoreComm<Hash, Block, Confirmation, Transaction>>,
            Receiver<CoreComm<Hash, Block, Confirmation, Transaction>>,
        ) = unbounded();
        let comms_transaction_service: (
            Sender<CoreComm<Hash, Block, Confirmation, Transaction>>,
            Receiver<CoreComm<Hash, Block, Confirmation, Transaction>>,
        ) = unbounded();

        CoreServices {
            block_service: comms_block_service.0,
            consensus_service: comms_consensus_service.0,
            drand_service: comms_drand_service.0,
            transaction_service: comms_transaction_service.0,
        }
    }

    fn mock_services_channels() -> (Services, Receivers) {
        let comms_db_service: (Sender<Comm>, Receiver<Comm>) = unbounded();
        let comms_event_service: (AsyncSender<Comm>, AsyncReceiver<Comm>) = async_unbounded();
        let comms_p2p_service: (AsyncSender<Comm>, AsyncReceiver<Comm>) = async_unbounded();
        let comms_wasm_service: (Sender<Comm>, Receiver<Comm>) = unbounded();

        let services = Services {
            db_service: comms_db_service.0,
            event_service: comms_event_service.0,
            p2p_service: comms_p2p_service.0,
            wasm_service: comms_wasm_service.0,
        };

        let receivers = Receivers {
            db_service: comms_db_service.1,
            event_service: comms_event_service.1,
            p2p_service: comms_p2p_service.1,
            wasm_service: comms_wasm_service.1,
        };

        (services, receivers)
    }

    fn start_event_service(services: Services, receivers: Receivers) {
        async_std::task::spawn(async move {
            EventService::start(
                async_unbounded().1,
                receivers.event_service,
                "test".to_string(),
                services,
            )
            .await
        });
    }

    fn start_socket_service(port: u16, services: Services) -> TrinciPublicKey {
        let keypair = TrinciKeyPair::new_ecdsa(
            CurveId::Secp384R1,
            concat!(env!("CARGO_MANIFEST_DIR"), "keypair.bin"),
        )
        .unwrap();

        let public_key = keypair.public_key();

        let node_info = NodeInfo {
            addr: "127.0.0.1".to_string(),
            public_key: public_key.clone(),
            mining_account_id: Some("test".into()),
            network: "test".into(),
            nonce: NONCE,
            rest_port: 8080,
        };

        async_std::task::spawn(SocketService::start(
            "127.0.0.1".to_string(),
            Some(Arc::new(keypair)),
            "test",
            node_info,
            mock_core_services(),
            services,
            port,
        ));

        public_key
    }

    #[async_std::test]
    async fn establish_socket_connection() {
        let (services, _) = mock_services_channels();

        start_socket_service(5000, services);

        async_std::task::sleep(std::time::Duration::from_millis(200)).await;
        let mut stream = TcpStream::connect("127.0.0.1:5000").await.unwrap();
        assert!(rmp_deserialize::<NodeInfo>(&read_datagram(&mut stream).await.unwrap(),).is_ok());
    }

    #[async_std::test]
    async fn concurrent_socket_connections() {
        let (services, _) = mock_services_channels();

        start_socket_service(5001, services);

        async_std::task::sleep(std::time::Duration::from_millis(200)).await;
        let mut streams = Vec::with_capacity(100);

        for _ in 0..100 {
            streams.push(TcpStream::connect("127.0.0.1:5001").await.unwrap());
        }

        assert!(streams.iter().all(|stream| stream.ttl().is_ok()));
    }

    #[async_std::test]
    async fn break_socket_connection() {
        let (services, _) = mock_services_channels();

        start_socket_service(5002, services);

        async_std::task::sleep(std::time::Duration::from_millis(200)).await;
        let stream1 = TcpStream::connect("127.0.0.1:5002").await.unwrap();
        assert!(stream1.shutdown(Shutdown::Both).is_ok());

        let mut stream2 = TcpStream::connect("127.0.0.1:5002").await.unwrap();
        assert!(rmp_deserialize::<NodeInfo>(&read_datagram(&mut stream2).await.unwrap(),).is_ok());
    }

    #[async_std::test]
    async fn event_subscription_ok() {
        let (services, receivers) = mock_services_channels();
        let services_clone = services.clone();
        start_event_service(services_clone, receivers);

        start_socket_service(5003, services);

        async_std::task::sleep(std::time::Duration::from_millis(200)).await;
        let mut stream = TcpStream::connect("127.0.0.1:5003").await.unwrap();

        let res = read_datagram(&mut stream).await.unwrap();
        assert!(rmp_deserialize::<NodeInfo>(&res,).is_ok());

        let req = SocketRequest::Subscribe {
            events: EventTopics::BLOCK_EXEC,
            id: "1".to_string(),
        };

        write_datagram(&mut stream, rmp_serialize(&req).unwrap())
            .await
            .unwrap();

        let res = read_datagram(&mut stream).await.unwrap();
        assert!(rmp_deserialize::<bool>(&res,).unwrap());

        let req = SocketRequest::Subscribe {
            events: EventTopics::BLOCK,
            id: "2".to_string(),
        };
        write_datagram(&mut stream, rmp_serialize(&req).unwrap())
            .await
            .unwrap();

        let res = read_datagram(&mut stream).await.unwrap();
        assert!(rmp_deserialize::<bool>(&res,).unwrap());

        let req = SocketRequest::Subscribe {
            events: EventTopics::CONTRACT_EVENTS,
            id: "3".to_string(),
        };
        write_datagram(&mut stream, rmp_serialize(&req).unwrap())
            .await
            .unwrap();

        let res = read_datagram(&mut stream).await.unwrap();
        assert!(rmp_deserialize::<bool>(&res,).unwrap());
    }

    // #[async_std::test]
    // #[cfg(feature = "mining")]
    // async fn tx_submission_ok() {
    //     let (services, receivers) = mock_services_channels();
    //     let public_key = start_socket_service(5004, services);

    //     async_std::task::sleep(std::time::Duration::from_millis(200)).await;
    //     let mut stream = TcpStream::connect("127.0.0.1:5004").await.unwrap();

    //     let res = read_datagram(&mut stream).await.unwrap();
    //     assert!(rmp_deserialize::<NodeInfo>(&res,).is_ok());

    //     let tx_data = TransactionDataV1 {
    //         account: "test".into(),
    //         fuel_limit: 0,
    //         nonce: vec![0],
    //         network: "test".into(),
    //         contract: None,
    //         method: "test".into(),
    //         caller: public_key,
    //         args: Vec::new(),
    //     };

    //     let req = SocketRequest::SubmitTx { tx_data };
    //     write_datagram(&mut stream, rmp_serialize(&req).unwrap())
    //         .await
    //         .unwrap();

    //     let res = read_datagram(&mut stream).await.unwrap();
    //     assert!(rmp_deserialize::<bool>(&res,).unwrap());

    //     if let Ok(Comm {
    //         kind: CommKind::Msg(Msg::PropagateTxToPeers(tx)),
    //         res_chan: _,
    //     }) = receivers.p2p_service.recv().await
    //     {
    //         assert!(tx.verify());
    //     }
    // }
}
