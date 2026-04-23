//! Peer delta update tracking.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use kameo::{
    actor::ActorRef,
    message::{Context, Message},
    reply::ReplySender,
};
use ts_control::{Node, NodeId};
use ts_keys::NodePublicKey;

use crate::{Error, env::Env};

/// Actor that tracks peer delta updates and emits new states.
pub struct PeerTracker {
    peers: HashMap<NodePublicKey, Node>,
    id_to_nodekey: HashMap<NodeId, NodePublicKey>,
    seen_state_update: bool,
    pending_requests: Vec<Pending>,
    env: Env,
}

impl PeerTracker {
    fn peer_by_name_opt(&self, name: &str) -> Option<&Node> {
        self.peers.values().find(|&peer| peer.matches_name(name))
    }
}

impl kameo::Actor for PeerTracker {
    type Args = Env;
    type Error = Error;

    async fn on_start(env: Self::Args, slf: ActorRef<Self>) -> Result<Self, Self::Error> {
        env.subscribe::<Arc<ts_control::StateUpdate>>(&slf).await?;

        Ok(Self {
            peers: Default::default(),
            id_to_nodekey: Default::default(),
            pending_requests: Default::default(),
            seen_state_update: false,
            env,
        })
    }
}

enum Pending {
    PeerByName(PeerByName, ReplySender<Option<Node>>),
}

// For messages with arguments, a struct is generated with the args as fields. They aren't
// documented, and we can't apply attributes directly to the fields. Hence, wrap in a module where
// docs are turned off everywhere.
#[allow(missing_docs)]
mod msg_impl {
    use kameo::prelude::DelegatedReply;

    use super::*;

    #[kameo::messages]
    impl PeerTracker {
        /// Lookup a peer by name.
        ///
        /// Waits until we've received at least one peer update from control.
        #[message(ctx)]
        pub async fn peer_by_name(
            &mut self,
            ctx: &mut Context<Self, DelegatedReply<Option<Node>>>,
            name: String,
        ) -> DelegatedReply<Option<Node>> {
            let (deleg, sender) = ctx.reply_sender();
            let Some(sender) = sender else { return deleg };

            if !self.seen_state_update {
                tracing::debug!(query = name, "no peer state seen yet, queueing request");

                self.pending_requests
                    .push(Pending::PeerByName(PeerByName { name }, sender));

                return deleg;
            }

            sender.send(self.peer_by_name_opt(&name).cloned());

            deleg
        }
    }
}

pub use msg_impl::*;

#[derive(Debug, Clone)]
pub(crate) struct PeerState {
    #[allow(unused)]
    pub deletions: HashSet<NodePublicKey>,
    #[allow(unused)]
    pub upserts: HashSet<NodePublicKey>,
    pub peers: Arc<HashMap<NodePublicKey, Node>>,
}

// TODO: rpds

impl Message<Arc<ts_control::StateUpdate>> for PeerTracker {
    type Reply = ();

    async fn handle(
        &mut self,
        msg: Arc<ts_control::StateUpdate>,
        _ctx: &mut Context<Self, Self::Reply>,
    ) {
        let Some(peer_update) = &msg.peer_update else {
            return;
        };

        let mut upserts = HashSet::default();
        let mut deletions = HashSet::default();

        match peer_update {
            ts_control::PeerUpdate::Full(nodes) => {
                tracing::trace!("full peer update");

                deletions = self.peers.keys().copied().collect();

                self.peers.clear();
                self.id_to_nodekey.clear();

                for node in nodes {
                    upserts.insert(node.node_key);
                    deletions.remove(&node.node_key);

                    self.id_to_nodekey.insert(node.id, node.node_key);
                    self.peers.insert(node.node_key, node.clone());
                }
            }

            ts_control::PeerUpdate::Delta { remove, upsert } => {
                tracing::trace!("delta peer update");

                for peer in upsert {
                    self.id_to_nodekey.insert(peer.id, peer.node_key);
                    self.peers.insert(peer.node_key, peer.clone());

                    upserts.insert(peer.node_key);
                }

                for peer in remove {
                    let node_key = self.id_to_nodekey.remove(peer);

                    if let Some(node_key) = node_key {
                        self.peers.remove(&node_key);
                        deletions.insert(node_key);
                    }
                }
            }
        }

        tracing::debug!(
            n_upsert = upserts.len(),
            n_delete = deletions.len(),
            peer_count = self.peers.len(),
            "new peer state"
        );

        if !self.seen_state_update {
            self.seen_state_update = true;

            if !self.pending_requests.is_empty() {
                tracing::debug!(
                    n_pending = self.pending_requests.len(),
                    "state update received, servicing pending requests"
                );
            }

            for req in core::mem::take(&mut self.pending_requests) {
                match req {
                    Pending::PeerByName(PeerByName { name }, reply) => {
                        reply.send(self.peer_by_name_opt(&name).cloned());
                    }
                }
            }
        }

        if let Err(e) = self
            .env
            .publish(PeerState {
                upserts,
                deletions,
                peers: Arc::new(self.peers.clone()),
            })
            .await
        {
            tracing::error!(error = %e, "publishing peer state update");
        }
    }
}
