//! The `drand_service` module is responsible for providing deterministic random number generation
//! within the blockchain system.  The module includes the `DRandService` struct for managing the DRand
//! logic and service communication.

use crate::{
    artifacts::{
        messages::{send_res, Comm, CommKind, Msg, Req, Res},
        models::{Confirmable, DRand, Executable, SeedSource, Transactable},
    },
    log_error,
};

use log::{debug, trace};

/// `DRandService` is a struct representing the service that manages `DRand` logic.
pub(crate) struct DRandService<A, B, C, T> {
    /// An instance of `DRand`, which is responsible for DRand numbers generation.
    drand: DRand,
    /// The identifier of the node in the network.
    node_id: String,
    _phantom: (
        std::marker::PhantomData<A>,
        std::marker::PhantomData<B>,
        std::marker::PhantomData<C>,
        std::marker::PhantomData<T>,
    ),
}

impl<
        A: Clone + Eq + std::hash::Hash + PartialEq + std::fmt::Debug + Send + 'static,
        B: Executable + Clone + std::fmt::Debug + Send + 'static,
        C: Confirmable + Clone + std::fmt::Debug + Send + 'static,
        T: Transactable + Clone + std::fmt::Debug + Send + 'static,
    > DRandService<A, B, C, T>
{
    /// The `start` function is responsible for initializing the service with a given `node_id` and `seed_source`, and then running it.
    pub(crate) fn start(
        drand_service_receiver: &crossbeam_channel::Receiver<Comm<A, B, C, T>>,
        node_id: String,
        seed_source: SeedSource,
    ) {
        let service = Self {
            drand: DRand::new(seed_source),
            node_id,
            _phantom: (
                std::marker::PhantomData,
                std::marker::PhantomData,
                std::marker::PhantomData,
                std::marker::PhantomData,
            ),
        };

        debug!("DRandService successfully started.");
        service.run(drand_service_receiver);
    }

    /// The `run` function is a method that continuously receives and handles communication with other services.
    fn run(mut self, drand_service_receiver: &crossbeam_channel::Receiver<Comm<A, B, C, T>>) {
        while let Ok(comm) = drand_service_receiver.recv() {
            match comm.kind.clone() {
                CommKind::Msg(msg) => {
                    self.handle_messages(&msg);
                }
                CommKind::Req(req) => {
                    if let Some(res_chan) = comm.res_chan {
                        self.handle_requests(&req, "DRandService", &res_chan);
                    } else {
                        log_error!(format!(
                            "Node {}, DRandService: received a request without response channel",
                            self.node_id,
                        ));
                    }
                }
            }
        }
    }

    /// The `handle_messages` function is responsible for handling incoming messages.
    #[inline]
    fn handle_messages(&mut self, msg: &Msg<A, B, C, T>) {
        match msg {
            Msg::UpdateSeedSource(last_block_hash, txs_hash, rxs_hash) => {
                debug!("Received `UpdateSeedSource` msg. New `last_bloch_hash` {}, `txs_hash` {}, `rxs_hash` {}", 
                    hex::encode(last_block_hash),
                    hex::encode(txs_hash),
                    hex::encode(rxs_hash)
                );
                self.drand
                    .update_seed_source(*last_block_hash, *txs_hash, *rxs_hash);
            }
            _ => {
                log_error!(format!(
                    "Node {}, DRandService: received unexpected Msg ({:?})",
                    self.node_id, msg
                ));
            }
        }
    }

    /// This function is responsible for handling incoming requests (`req`) from a specified service (`from_service`).
    /// It uses a response channel (`res_chan`) to send responses back to the service.
    #[inline]
    fn handle_requests(
        &mut self,
        req: &Req,
        from_service: &str,
        res_chan: &crossbeam_channel::Sender<Res<A, B, C, T>>,
    ) {
        match req {
            Req::GetDRandNum(max) => {
                debug!(
                    "Received `GetRandNum` req from {}. Max number extractable {}",
                    from_service, max
                );
                trace!("`Drand` actual seed: {:?}", self.drand.seed);

                self.drand = DRand::new(self.drand.seed.clone());
                let drand_num = self.drand.rand(*max);
                trace!(
                    "Extracted random number for service {}: {}",
                    from_service,
                    drand_num
                );

                send_res(
                    &self.node_id,
                    "DRandService",
                    from_service,
                    res_chan,
                    Res::GetDRandNum(drand_num),
                );
            }
            _ => {
                log_error!(format!(
                    "Node {}, DRandService: received unexpected Req ({:?})",
                    self.node_id, req
                ));
            }
        }
    }
}
