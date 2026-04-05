use core::net::{IpAddr, SocketAddr};

use chrono::{DateTime, Utc};
use ts_keys::{DiscoPublicKey, MachinePublicKey, NodePublicKey};

/// The unique id of a node.
pub type Id = i64;

/// The stable ID of a node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StableId(String);

/// A node in a tailnet.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Node {
    /// The node's id.
    pub id: Id,
    /// The node's stable id.
    pub stable_id: StableId,
    /// The name of the node.
    pub name: String,
    /// The tags assigned to this node.
    pub tags: Vec<String>,

    /// The address of the node in the tailnet.
    pub tailnet_address: TailnetAddress,

    /// The node's [`NodePublicKey`].
    pub node_key: NodePublicKey,
    /// The node key's expiration.
    pub node_key_expiry: Option<DateTime<Utc>>,

    /// The node's [`MachinePublicKey`], if known.
    pub machine_key: Option<MachinePublicKey>,
    /// The node's [`DiscoPublicKey`], if known.
    pub disco_key: Option<DiscoPublicKey>,

    /// The routes this node accepts traffic for.
    pub accepted_routes: Vec<ipnet::IpNet>,
    /// The underlay addresses this node is reachable on (`Endpoints` in Go).
    pub underlay_addresses: Vec<SocketAddr>,

    /// The DERP region for this node, if known.
    pub derp_region: Option<ts_transport_derp::RegionId>,
}

/// Addresses for a node within a tailnet.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TailnetAddress {
    /// The IPv4 address of the node in the tailnet.
    pub ipv4: ipnet::Ipv4Net,
    /// The IPv6 address of the node in the tailnet.
    pub ipv6: ipnet::Ipv6Net,
}

impl TailnetAddress {
    /// Report whether `addr` matches either address in this [`TailnetAddress`].
    pub fn contains(&self, addr: IpAddr) -> bool {
        match addr {
            IpAddr::V4(a) => self.ipv4.addr() == a,
            IpAddr::V6(a) => self.ipv6.addr() == a,
        }
    }
}

impl From<&ts_control_serde::Node<'_>> for Node {
    fn from(value: &ts_control_serde::Node) -> Self {
        Self {
            id: value.id,
            stable_id: StableId(value.stable_id.0.to_string()),
            name: value.name.to_string(),
            tags: value
                .tags
                .as_ref()
                .map(|x| x.iter().map(|x| x.to_string()).collect())
                .unwrap_or_default(),

            tailnet_address: TailnetAddress {
                ipv4: value.addresses.0,
                ipv6: value.addresses.1,
            },
            node_key: value.key,
            node_key_expiry: value.key_expiry,
            machine_key: value.machine,
            disco_key: value.disco_key,

            accepted_routes: value
                .allowed_ips
                .clone()
                .unwrap_or_else(|| vec![value.addresses.0.into(), value.addresses.1.into()]),
            underlay_addresses: value.endpoints.clone(),

            // legacy_derp_string is still in practical use as of 3/2026
            #[allow(deprecated)]
            derp_region: value
                .home_derp
                .or(value.legacy_derp_string)
                .or_else(|| value.host_info.net_info.as_ref()?.preferred_derp)
                .map(|x| ts_transport_derp::RegionId(x.into())),
        }
    }
}
