//! The `p2p_service` module implements the library `libp2p`. The module uses tho main communication protocols: `gossipsub` and `request_response`
//! to handle communication between peers for broadcast and unicast communication respectively.

use std::{collections::VecDeque, net::IpAddr, str::FromStr};

use crate::{
    artifacts::{
        errors::CommError,
        messages::{
            Comm, CommKind, ExtComm, Msg, Payload, Req, ReqUnicastMessage, Res, ResUnicastMessage,
            TrinciTopics, TRINCI_TOPICS,
        },
        models::{Block, Confirmation, P2PKeyPair, Transaction},
    },
    consts::{MAX_KNOWN_PEERS, MAX_TRANSACTION_SIZE, PEER_KEEP_ALIVE_S, QUERY_TIMEOUT_S},
    utils::{peer_id_from_str, retrieve_attachments_from_tx},
    Services,
};

use async_std::task::sleep;
use systemstat::Duration;
use trinci_core_new::{
    artifacts::{
        messages::{
            send_msg, send_req, send_res, Comm as CoreComm, CommKind as CoreCommKind,
            Msg as CoreMsg,
        },
        models::{Services as CoreServices, Transactable},
    },
    crypto::hash::Hash,
    log_error,
    utils::{random_number_in_range_from_zero, rmp_deserialize, rmp_serialize},
};

use futures::{future::Either, StreamExt};
use libp2p::{
    autonat,
    gossipsub::{self, MessageId},
    identify,
    kad::{self, store::MemoryStore, Config},
    noise,
    request_response::{self, OutboundRequestId, ProtocolSupport, ResponseChannel},
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder,
};
use log::{debug, info, trace, warn};
pub struct Peer {
    pub addresses: Vec<String>,
    pub peer_id: String,
}

pub struct LatestKnownPeers {
    pub bootstrap_peer: Option<Peer>,
    pub latest_peers: VecDeque<Peer>,
}

impl LatestKnownPeers {
    fn new(bootstrap_addresses: Option<Vec<String>>, bootstrap_peer_id: Option<String>) -> Self {
        let bootstrap_peer = if let (Some(bootstrap_addresses), Some(peer_id)) =
            (bootstrap_addresses, bootstrap_peer_id)
        {
            Some(Peer {
                addresses: bootstrap_addresses,
                peer_id,
            })
        } else {
            None
        };

        Self {
            bootstrap_peer,
            latest_peers: VecDeque::with_capacity(MAX_KNOWN_PEERS),
        }
    }
}

/// Our network behavior.
#[derive(NetworkBehaviour)]
pub struct Behaviour {
    pub autonat: autonat::Behaviour,
    pub gossipsub: gossipsub::Behaviour,
    pub identify: identify::Behaviour,
    pub kademlia: kad::Behaviour<MemoryStore>,
    pub reqres: request_response::Behaviour<ExtComm>,
}

impl Behaviour {
    #[must_use]
    pub fn new(
        keypair: P2PKeyPair,
        addresses: Option<Vec<String>>,
        peer_id: Option<String>,
        network_name: String,
    ) -> Self {
        // Create a Kademlia behavior.
        let mut cfg = Config::default()
            .set_protocol_names(vec![StreamProtocol::try_from_owned(format!(
                "/trinci/{network_name}/1.0.0"
            ))
            .expect("The protocol format should always be good")])
            .to_owned();
        cfg.set_query_timeout(std::time::Duration::from_secs(QUERY_TIMEOUT_S));
        let store = MemoryStore::new(keypair.get_peer_id());
        let mut kademlia = kad::Behaviour::with_config(keypair.get_peer_id(), store, cfg);

        // Adds a boot peer to the DHT.
        // Adds a known listen address of a peer participating in the DHT to the routing table.
        // Once a boot peer is added, start bootstrap
        if let (Some(addresses), Some(peer_id)) = (addresses, peer_id) {
            debug!("Adding P2P bootstrap node to Kademlia DHT");
            for addr in addresses {
                kademlia.add_address(
                    &peer_id
                        .parse()
                        .expect("Expected peer_id to be valid UTF-8 string"),
                    addr.parse()
                        .expect("Expected addr to be a correctly formatted address string"),
                );

                debug!("Added {addr} to kad DHT");

                if let Err(e) = kademlia.bootstrap() {
                    log_error!(format!("Error during kademlia bootstrap. Reason: {e}"));
                }
            }
        }

        let mut cfg = identify::Config::new("/trinci/1.0.0".into(), keypair.keypair.public());
        cfg.push_listen_addr_updates = true;

        let identify = identify::Behaviour::new(cfg);

        // Create a RequestResponse behavior.
        let reqres = request_response::Behaviour::with_codec(
            ExtComm(),
            vec![(ExtComm {}, ProtocolSupport::Full)],
            request_response::Config::default(),
        );

        let config = gossipsub::ConfigBuilder::default()
            .max_transmit_size(MAX_TRANSACTION_SIZE)
            .build()
            .expect("GossipSub builder should always work, it is using inner methods to be built");

        let autonat = autonat::Behaviour::new(
            keypair.get_peer_id(),
            autonat::Config {
                retry_interval: std::time::Duration::from_secs(90),
                refresh_interval: std::time::Duration::from_secs(15 * 60),
                boot_delay: std::time::Duration::from_secs(15),
                throttle_server_period: std::time::Duration::from_secs(90),
                only_global_ips: false, // TODO: check this flag
                ..Default::default()
            },
        );

        Self {
            autonat,
            // The `new` function from `gossipsub::Behaviour` can only fail when the
            // `MessageAuthenticity` variant is `SignedBroadcast`. Since we're using
            // `Signed` variant here, the call will never fail. Hence, it's safe to use
            // `unwrap()` here.
            gossipsub: gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(keypair.keypair),
                config,
            )
            .unwrap(),
            identify,
            kademlia,
            reqres,
        }
    }
}

pub struct P2PService {
    pub core_services: CoreServices<Hash, Block, Confirmation, Transaction>,
    pub node_id: String,
    pub network_name: String,
    pub kbucket_key: libp2p::kad::KBucketKey<PeerId>,
    pub public_address: Option<String>,
    pub pending_req:
        std::collections::HashMap<OutboundRequestId, Option<crossbeam_channel::Sender<Res>>>,
    pub services: Services,
    pub swarm: Swarm<Behaviour>,
    // This should contain the latest `k` known peers. (For now just bootstrap peer)
    pub latest_known_peers: LatestKnownPeers,
}

impl P2PService {
    pub fn start(
        bootstrap_addresses: Option<Vec<String>>,
        bootstrap_peer_id: Option<String>,
        core_services: CoreServices<Hash, Block, Confirmation, Transaction>,
        interfaces: Vec<String>,
        port: u16,
        p2p_service_receiver: async_std::channel::Receiver<Comm>,
        public_address: Option<String>,
        keypair: P2PKeyPair,
        node_id: String,
        network_name: String,
        peer_id: PeerId,
        services: Services,
    ) {
        let mut swarm = SwarmBuilder::with_existing_identity(keypair.keypair.clone())
            .with_async_std()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|e| log_error!(format!("Error during P2P TCP setup, reason: {}", e)))
            .unwrap()
            .with_behaviour(|_| {
                Behaviour::new(
                    keypair,
                    bootstrap_addresses.clone(),
                    bootstrap_peer_id.clone(),
                    network_name.clone(),
                )
            })
            .map_err(|e| log_error!(format!("Error during P2P behaviour setup, reason: {}", e)))
            .unwrap()
            .with_swarm_config(|c| {
                c.with_idle_connection_timeout(std::time::Duration::from_secs(PEER_KEEP_ALIVE_S))
            })
            .build();

        // Associate the swarm to all the possible interfaces and get a Os-assigned port.
        for interface in interfaces {
            if let Err(e) = swarm.listen_on(format!("/ip4/{}/tcp/{}", interface, port).parse().expect("The address is written in a bad format, it should be /<ip-protocol>/<address>/<tcp/upd protocol>/<port>")) {
            log_error!(format!("Error when starting listening on /ip4/{interface}/tcp/{port}. Reason: {e}"));
        };
        }

        // Adding public ip as external address.
        if let Some(public_ip) = &public_address {
            let multiaddr = Multiaddr::from_str(&format!("/ip4/{public_ip}/tcp/{port}"))
                .expect("Multiaddress form should be `/ip4/[addr]/tcp/[port]`");

            info!("New listen addr: {:?}", multiaddr);

            swarm.add_external_address(multiaddr);
        }

        // In case a bootstrap peer is available, the node use it as autoNAT server
        // to infer if whether or not is reachable outside is local network.
        if let (Some(peer_id), Some(peer_addresses)) = (&bootstrap_peer_id, &bootstrap_addresses) {
            for address in peer_addresses {
                swarm.behaviour_mut().autonat.add_server(
                    PeerId::from_str(peer_id).unwrap(),
                    Multiaddr::from_str(address).ok(),
                );
            }
        }

        for topic in TRINCI_TOPICS {
            swarm
                .behaviour_mut()
                .gossipsub
                .subscribe(&gossipsub::IdentTopic::new(topic.to_string()))
                .expect("Failed to subscribe to topic {topic}");
        }

        let latest_known_peers = LatestKnownPeers::new(bootstrap_addresses, bootstrap_peer_id);
        let service = P2PService {
            core_services,
            node_id,
            network_name,
            kbucket_key: libp2p::kad::KBucketKey::from(peer_id),
            pending_req: std::collections::HashMap::new(),
            services,
            swarm,
            public_address,
            latest_known_peers,
        };

        info!("P2PService successfully started.");
        async_std::task::spawn(service.run(p2p_service_receiver));
    }

    async fn run(mut self, p2p_service_receiver: async_std::channel::Receiver<Comm>) {
        loop {
            match futures::future::select(
                self.swarm.select_next_some(),
                p2p_service_receiver.recv(),
            )
            .await
            {
                Either::Left((p2p_event, _)) => {
                    match p2p_event {
                        SwarmEvent::Behaviour(behaviour_event) => {
                            self.handle_behaviour_event(behaviour_event);
                        }
                        SwarmEvent::ConnectionClosed { peer_id, .. } => {
                            self.swarm.behaviour_mut().kademlia.remove_peer(&peer_id);
                            debug!("Connection closed with {}", peer_id.to_string());

                            if self.swarm.connected_peers().count() == 0
                                && !self.latest_known_peers.latest_peers.is_empty()
                            {
                                warn!("The peer has no P2P connections, trying to reconnect to latest known peers");
                                self.reconnect_to_network().await;
                            }
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            debug!("Connection established with {}", peer_id.to_string());
                        }
                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!("New listen addr: {:?}", address);

                            // By adding an external adders the lib express
                            // an address from which any peer can contact
                            // the local peer for any request.
                            // Additionally the local peer becomes a SERVER
                            // so it can handle any inbound request.
                            // NOTE: without this kad wont work.
                            self.swarm.add_external_address(address);
                        }
                        _ => {}
                    }
                }
                Either::Right((int_comm, _)) => {
                    if let Ok(int_comm) = int_comm {
                        #[cfg(feature = "standalone")]
                        self.handle_int_comm(int_comm);
                    }
                }
            }
        }
    }

    #[inline]
    fn handle_behaviour_event(&mut self, behaviour_event: BehaviourEvent) {
        match behaviour_event {
            BehaviourEvent::Autonat(autonat_event) => {
                trace!("AutoNAT event received: {autonat_event:?}");
            }
            BehaviourEvent::Gossipsub(gossipsub::Event::Message { message, .. }) => {
                if let Ok(payload) = rmp_deserialize(&message.data) {
                    self.handle_gossip_msg(message.source, payload);
                } else {
                    let source = match message.source {
                        Some(peer_id) => peer_id.to_string(),
                        None => "not found".to_string(),
                    };

                    warn!(
                        "Gossip message transport an wrongly serialized message. Source {source}"
                    );
                }
            }
            BehaviourEvent::Gossipsub(gossip_event) => {
                trace!("Gossipsub event received: {gossip_event:?}");
            }
            BehaviourEvent::Identify(identify_event) => {
                self.handle_identify_event(&identify_event);
            }
            BehaviourEvent::Kademlia(kad_event) => {
                trace!("Kademlia event received {kad_event:?}");
            }
            BehaviourEvent::Reqres(reqres_event) => {
                self.handle_reqres_event(reqres_event);
            }
        }
    }

    #[inline]
    #[allow(unreachable_patterns)]
    fn handle_gossip_msg(&mut self, source: Option<PeerId>, gossip_msg: Payload) {
        // Collecting peer source, needed for debug logging.
        let from = match source {
            Some(peer) => format!(" from {peer}"),
            None => "".to_string(),
        };

        match gossip_msg {
            Payload::Confirmation(confirmation) => {
                debug!("Received gossip `Confirmation` msg{from}");
                send_msg(
                    &self.node_id,
                    "P2PService",
                    "ConsensusService",
                    &self.core_services.consensus_service,
                    CoreComm::new(
                        CoreCommKind::Msg(
                            CoreMsg::<Hash, Block, Confirmation, Transaction>::BlockConfirmationFromPeer(
                                confirmation,
                            ),
                        ),
                        None,
                    ),
                );
            }
            Payload::Block(block, confirmation, txs_hashes) => {
                debug!("Received gossip `Block` msg{from}");
                send_msg(
                    &self.node_id,
                    "P2PService",
                    "ConsensusService",
                    &self.core_services.consensus_service,
                    CoreComm::new(
                        CoreCommKind::Msg(
                            CoreMsg::<Hash, Block, Confirmation, Transaction>::BlockFromPeer(
                                block,
                                confirmation,
                                txs_hashes,
                            ),
                        ),
                        None,
                    ),
                );
            }
            Payload::OldTransactions(txs) => {
                debug!("Received gossip `OldTransactions` msg{from}");

                // Verify if already in DB.
                let txs_hashes = txs
                    .iter()
                    .map(trinci_core_new::artifacts::models::Transactable::get_hash)
                    .collect();
                let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
                let Ok(Res::HasTxs(mask)) = send_req::<Comm, Res, CommError<Comm>>(
                    &self.node_id,
                    "P2PService",
                    "DBService",
                    &self.services.db_service,
                    Comm::new(CommKind::Req(Req::HasTxs(txs_hashes)), Some(res_sender)),
                    &res_receiver,
                ) else {
                    warn!("Problem encountered during `HasTxs` req to DBService.");
                    return;
                };

                // In case transactions are not present in the DB,
                // send those to the TransactionService.
                let txs: Vec<(Transaction, usize, Option<Vec<Hash>>)> = txs
                    .into_iter()
                    .zip(mask)
                    .filter_map(|(tx, in_db)| if !in_db { Some(tx) } else { None })
                    .map(|tx| {
                        (
                            tx.to_owned(),
                            rmp_serialize(&tx)
                                .expect("Transaction should always be serializable")
                                .len(),
                            retrieve_attachments_from_tx(&tx),
                        )
                    })
                    .collect();

                send_msg(
                    &self.node_id,
                    "P2PService",
                    "TransactionService",
                    &self.core_services.transaction_service,
                    CoreComm::new(CoreCommKind::Msg(CoreMsg::AddTxs(txs)), None),
                );
            }
            Payload::Transaction(tx) => {
                debug!("Received gossip `Transaction` msg {from}");

                if tx.get_network() != self.network_name {
                    warn!("Received gossip `Transaction` msg contains a transaction with another network");
                    return;
                }

                // Verify if already in DB
                let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
                if let Ok(Res::HasTx(false)) = send_req::<Comm, Res, CommError<Comm>>(
                    &self.node_id,
                    "P2PService",
                    "DBService",
                    &self.services.db_service,
                    Comm::new(CommKind::Req(Req::HasTx(tx.get_hash())), Some(res_sender)),
                    &res_receiver,
                ) {
                    // Calculates tx dimension.
                    let size = rmp_serialize(&tx)
                        .expect("Transaction should always be serializable")
                        .len();

                    let attachments = retrieve_attachments_from_tx(&tx);

                    send_msg(
                        &self.node_id,
                        "P2PService",
                        "TransactionService",
                        &self.core_services.transaction_service,
                        CoreComm::new(
                            CoreCommKind::Msg(CoreMsg::AddTx((tx, size, attachments))),
                            None,
                        ),
                    );
                }
            }
            _ => {}
        }
    }

    #[inline]
    // Since Request is defined in our local scope and its serialization is
    // implemented by us, rmp_serialize is highly unlikely to fail. Thus,
    // unwrapping is safe.
    fn handle_identify_event(&mut self, event: &identify::Event) {
        if let libp2p::identify::Event::Received { peer_id, info } = event {
            trace!("Identify event received from {peer_id}");

            // Peer Discovery with Identify In other libp2p implementations, the Identify protocol might be seen as a core protocol.
            // Rust-libp2p tries to stay as generic as possible, and does not make this assumption.
            // This means that the Identify protocol must be manually hooked up to Kademlia through calls
            // to Behaviour::add_address. If you choose not to use the Identify protocol,
            // and do not provide an alternative peer discovery mechanism, a Kademlia node will not discover nodes beyond the network's boot nodes.
            // Without the Identify protocol, existing nodes in the kademlia network cannot obtain the listen addresses of nodes querying them,
            // and thus will not be able to add them to their routing table.
            for addr in info.listen_addrs.clone() {
                self.swarm
                    .behaviour_mut()
                    .kademlia
                    .add_address(peer_id, addr);
            }

            // Adding the peer in the latest known peers, so to have a list of possible addresses to contact
            // in case the local peer disconnects from the network. In that chance the local peer will dial
            // the latest_known_peers and try to add those again in case it can dial them.
            if self.latest_known_peers.latest_peers.len() == MAX_KNOWN_PEERS {
                self.latest_known_peers.latest_peers.pop_front();
            }
            self.latest_known_peers.latest_peers.push_back(Peer {
                addresses: info
                    .listen_addrs
                    .iter()
                    .map(|addr| addr.to_string())
                    .collect(),
                peer_id: peer_id.to_string(),
            });

            let req_id = self.swarm.behaviour_mut().reqres.send_request(
                peer_id,
                ReqUnicastMessage(rmp_serialize(&Req::GetHeight).unwrap()),
            );
            self.pending_req.insert(req_id, None);
        }
    }

    #[inline]
    fn handle_reqres_event(
        &mut self,
        event: libp2p::request_response::Event<ReqUnicastMessage, ResUnicastMessage>,
    ) {
        match event {
            request_response::Event::Message { peer: _, message } => match message {
                request_response::Message::Request {
                    request_id: _,
                    request,
                    channel,
                } => {
                    self.handle_peer_requests(&request, channel);
                }
                request_response::Message::Response {
                    request_id,
                    response,
                } => self.handle_peer_response(&response, request_id),
            },
            request_response::Event::OutboundFailure {
                peer,
                request_id,
                error,
            } => {
                warn!(
                    "ReqRes OutboundFailure event. Peer: {peer:?} - Request id: {request_id:?}. Reason: {error:?}",
                );
                self.pending_req.remove(&request_id);
            }
            request_response::Event::InboundFailure {
                peer,
                request_id,
                error,
            } => {
                warn!(
                        "ReqRes InboundFailure event. Peer: {peer:?} - Request id: {request_id:?}. Reason: {error:?}",
                    );
            }
            request_response::Event::ResponseSent { peer, request_id } => {
                debug!("ReqRes ResponseSent event. Peer: {peer:?} - Request id: {request_id:?}",);
            }
        }
    }

    // Since Response is defined in our local scope and its serialization is
    // implemented by us, rmp_serialize is highly unlikely to fail. Thus,
    // unwrapping is safe.
    #[inline]
    fn handle_peer_requests(
        &mut self,
        req: &ReqUnicastMessage,
        res_chan: ResponseChannel<ResUnicastMessage>,
    ) {
        let Ok(req) = rmp_deserialize(&req.0) else {
            log_error!(format!("Error during request (req:?) deserialization"));
            return;
        };

        trace!("Received request from P2P peer {req:?}");

        let (res_sender, res_receiver) = crossbeam_channel::bounded(2);
        if let Ok(response) = send_req::<Comm, Res, CommError<Comm>>(
            &self.node_id,
            "P2PService",
            "DBService",
            &self.services.db_service,
            Comm::new(CommKind::Req(req), Some(res_sender)),
            &res_receiver,
        ) {
            trace!("Sending response to P2P peer");
            let res = ResUnicastMessage(rmp_serialize(&response).unwrap());

            if let Err(error) = self
                .swarm
                .behaviour_mut()
                .reqres
                .send_response(res_chan, res)
            {
                warn!(
                    "Node {}, P2PService: failed to send response to peer. Reason: {:?}",
                    &self.node_id, error
                );
            }
        };
    }

    #[inline]
    fn handle_peer_response(
        &mut self,
        response: &ResUnicastMessage,
        request_id: OutboundRequestId,
    ) {
        let Ok(res) = rmp_deserialize::<Res>(&response.0) else {
            log_error!(format!(
                "Error during response ({response:?}) deserialization"
            ));
            return;
        };
        let pending_res = self.pending_req.remove(&request_id);

        match &pending_res {
            Some(Some(res_sender)) => {
                if let Err(e) = res_sender.send(res.clone()) {
                    warn!("Problems when sending {res:?} from P2PService. Reason: {e}");
                }
            }
            Some(None) => {
                if let Res::GetHeight(height) = res {
                    send_msg(
                        &self.node_id,
                        "P2PService",
                        "BlockService",
                        &self.core_services.block_service,
                        CoreComm::new(CoreCommKind::Msg(CoreMsg::HeightFromPeer(height)), None),
                    );
                }
            }
            None => {
                warn!("Node {}, P2PService: received response {response:?} relative to request with id {request_id:?} that was not found in pending requests", self.node_id);
            }
        }
    }

    #[inline]
    fn handle_int_comm(&mut self, int_comm: Comm) {
        match int_comm.kind.clone() {
            CommKind::Msg(msg) => {
                self.handle_messages(msg);
            }
            CommKind::Req(req) => {
                self.handle_requests(
                    &req,
                    int_comm
                        .res_chan
                        .expect("Here I should always have a channel"),
                );
            }
        }
    }

    #[inline]
    // Since Payload is defined in our local scope and its serialization is
    // implemented by us, rmp_serialize is highly unlikely to fail. Thus,
    // unwrapping is safe.
    fn handle_messages(&mut self, msg: Msg) {
        let publish_result = match msg {
            Msg::PropagateBlockToPeers {
                block,
                confirmation,
                txs_hashes,
            } => {
                debug!("Received `PropagateBlockToPeers` msg.");
                self.swarm.behaviour_mut().gossipsub.publish(
                    gossipsub::IdentTopic::new(TrinciTopics::Blocks.to_string()),
                    rmp_serialize(&Payload::Block(block, confirmation, txs_hashes)).unwrap(),
                )
            }
            Msg::PropagateConfirmationToPeers(confirmation) => {
                debug!("Received `PropagateConfirmationToPeers` msg.");
                self.swarm.behaviour_mut().gossipsub.publish(
                    gossipsub::IdentTopic::new(TrinciTopics::Consensus.to_string()),
                    rmp_serialize(&Payload::Confirmation(confirmation)).unwrap(),
                )
            }
            Msg::PropagateOldTxs(txs) => {
                debug!("Received `PropagateOldTxs` msg.");
                for tx in txs {
                    if let Err(e) = self.swarm.behaviour_mut().gossipsub.publish(
                        gossipsub::IdentTopic::new(TrinciTopics::Txs.to_string()),
                        rmp_serialize(&Payload::OldTransactions(vec![tx])).unwrap(),
                    ) {
                        warn!("Problem encountered during gossip publication. Reason: {e}");
                    }
                }
                Ok(MessageId::new(&[0]))
            }
            Msg::PropagateTxToPeers(tx) => {
                debug!("Received `PropagateTxToPeers` msg.");
                self.swarm.behaviour_mut().gossipsub.publish(
                    gossipsub::IdentTopic::new(TrinciTopics::Txs.to_string()),
                    rmp_serialize(&Payload::Transaction(tx)).unwrap(),
                )
            }
            _ => {
                log_error!(format!(
                    "Node {}, P2PService: received unexpected Msg ({msg:?})",
                    self.node_id,
                ));
                return;
            }
        };

        if let Err(e) = publish_result {
            warn!("Problem encountered during gossip publication. Reason: {e}");
        }
    }

    #[inline]
    // Since Request is defined in our local scope and its serialization is
    // implemented by us, rmp_serialize is highly unlikely to fail. Thus,
    // unwrapping is safe.
    fn handle_requests(&mut self, req: &Req, res_chan: crossbeam_channel::Sender<Res>) {
        match req {
            Req::GetFullBlock {
                ref trusted_peers, ..
            } => {
                debug!("Received `GetFullBlock` req.");
                let neighbor =
                    &trusted_peers[random_number_in_range_from_zero(trusted_peers.len() - 1)];
                let peer_id = peer_id_from_str(neighbor).unwrap(); // Neighbor is stringified from a P2P PeerId, safe to unwrap

                let req_id = self
                    .swarm
                    .behaviour_mut()
                    .reqres
                    .send_request(&peer_id, ReqUnicastMessage(rmp_serialize(&req).unwrap()));
                self.pending_req.insert(req_id, Some(res_chan));
            }
            Req::GetLastBlockInfo(ref neighbor) => {
                debug!("Received `GetLastBlockInfo` req.");
                let peer_id = peer_id_from_str(neighbor).unwrap(); // Neighbor is stringified from a P2P PeerId, safe to unwrap

                let req_id = self
                    .swarm
                    .behaviour_mut()
                    .reqres
                    .send_request(&peer_id, ReqUnicastMessage(rmp_serialize(&req).unwrap()));
                self.pending_req.insert(req_id, Some(res_chan));
            }
            Req::GetNeighbors => {
                debug!("Received `GetNeighbors` req.");
                let neighbors = self
                    .swarm
                    .behaviour_mut()
                    .kademlia
                    .get_closest_local_peers(&self.kbucket_key)
                    .take(crate::consts::NUM_OF_NEIGHBORS)
                    .map(|peer_id| peer_id.preimage().to_string())
                    .collect();

                send_res(
                    &self.node_id,
                    "P2PService",
                    "CoreInterface",
                    &res_chan,
                    Res::GetNeighbors(neighbors),
                );
            }
            Req::GetP2pAddresses => {
                debug!("Received `GetP2pAddresses` req.");
                let addresses: Vec<String> = self
                    .swarm
                    .external_addresses()
                    .map(|addr| addr.to_string())
                    .collect();

                // Removes the `/p2p/` address part added automatically.
                let addresses = addresses
                    .into_iter()
                    .map(|address| address.split("/p2p/").next().unwrap().to_string()) // Unwrap is safe because it is always present at least the part /ip4/[addr]/tcp/[port]
                    .collect();

                send_res(
                    &self.node_id,
                    "P2PService",
                    "RestAPI",
                    &res_chan,
                    Res::GetP2pAddresses(addresses),
                );
            }
            Req::GetTxs(_) | Req::GetConfirmations { .. } => {
                debug!("Received `GetTxs` req.");
                let peer_id = self.get_random_neighbor();

                let req_id = self
                    .swarm
                    .behaviour_mut()
                    .reqres
                    .send_request(&peer_id, ReqUnicastMessage(rmp_serialize(&req).unwrap()));
                self.pending_req.insert(req_id, Some(res_chan));
            }
            _ => {
                log_error!(format!(
                    "Node {}, P2PService: received unexpected Req ({:?})",
                    self.node_id, req
                ));
            }
        }
    }

    #[inline]
    fn get_random_neighbor(&mut self) -> PeerId {
        let neighbors: Vec<_> = self
            .swarm
            .behaviour_mut()
            .kademlia
            .get_closest_local_peers(&self.kbucket_key)
            .take(crate::consts::NUM_OF_NEIGHBORS)
            .collect();

        *neighbors[trinci_core_new::utils::random_number_in_range_from_zero(
            neighbors.len().min(crate::consts::NUM_OF_NEIGHBORS) - 1,
        )]
        .preimage()
    }

    async fn reconnect_to_network(&mut self) {
        // Until the peer can't reach the bootstrap peer do not try to add it in the swarm
        let mut random_peer = if let Some(peer) = &self.latest_known_peers.bootstrap_peer {
            peer
        } else {
            &self.latest_known_peers.latest_peers
                [rand::random::<usize>() % self.latest_known_peers.latest_peers.len()]
        };

        let mut reachable = false;
        while !reachable {
            // Try to deal a peer from its external address
            let multi_addresses: Vec<Multiaddr> = random_peer
                .addresses
                .iter()
                .filter(|addr| {
                    // The needed addresses are only the public ones.
                    // `/ip4/127.0.0.1/tcp/6767` -> third element is the ip
                    let sub_string: Vec<&str> = addr.split_terminator('/').collect();
                    is_public_ip(sub_string[2])
                })
                .map(|addr| {
                    addr.parse::<Multiaddr>()
                        .expect("This String should always represent a multiaddr")
                })
                .collect();

            for addr in multi_addresses {
                if self.swarm.dial(addr).is_ok() {
                    // if dial is ok it meas that node is connected to the public network
                    reachable = true;
                }
            }

            sleep(Duration::from_secs(10)).await;

            // If the peer can't be reached it tries with another one
            random_peer = if let Some(peer) = &self.latest_known_peers.bootstrap_peer {
                peer
            } else {
                (&self.latest_known_peers.latest_peers
                    [rand::random::<usize>() % self.latest_known_peers.latest_peers.len()])
                    as _
            };
        }

        // If outside the loop it means that the local peer can reach the pubic network,
        // so it adds the peer to rejoin the peer network
        random_peer.addresses.iter().for_each(|addr| {
            self.swarm.add_peer_address(
                PeerId::from_str(&random_peer.peer_id)
                    .expect("This should always be a good peer ID"),
                Multiaddr::from_str(addr).expect("This should alway be a good addr"),
            )
        })
    }
}

fn is_public_ip(ip: &str) -> bool {
    match ip.parse::<IpAddr>() {
        Ok(addr) => match addr {
            IpAddr::V4(ipv4) => {
                // Private IPv4 ranges
                let private_ranges = [
                    (10, 8),               // 10.0.0.0/8
                    (172 * 256 + 16, 12),  // 172.16.0.0/12
                    (192 * 256 + 168, 16), // 192.168.0.0/16
                ];
                let ip_as_u32 = u32::from(ipv4);
                for &(base, mask) in &private_ranges {
                    if ip_as_u32 >> (32 - mask) == base {
                        return false;
                    }
                }
                true
            }
            IpAddr::V6(ipv6) => {
                // Private IPv6 range: fc00::/7
                if ipv6.segments()[0] & 0xfe00 == 0xfc00 {
                    return false;
                }
                // Link-local IPv6 range: fe80::/10
                if ipv6.segments()[0] & 0xffc0 == 0xfe80 {
                    return false;
                }
                true
            }
        },
        Err(_) => false,
    }
}
