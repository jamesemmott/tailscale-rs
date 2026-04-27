use ts::keys::{DiscoPrivateKey, MachinePrivateKey, NetworkLockPrivateKey, NodePrivateKey};

/// Tailscale keys.
#[derive(Debug, Clone, PartialEq, Eq)]
#[pyo3::pyclass(frozen, get_all, from_py_object, module = "tailscale")]
pub struct Keystate {
    /// Machine key.
    pub machine: Vec<u8>,
    /// Node (device) key.
    pub node: Vec<u8>,
    /// Disco key.
    pub disco: Vec<u8>,
    /// Network lock key.
    pub network_lock: Vec<u8>,
}

#[pyo3::pymethods]
impl Keystate {
    #[new]
    #[pyo3(signature = (machine=None, node=None, disco=None, network_lock=None))]
    pub fn new(
        machine: Option<Vec<u8>>,
        node: Option<Vec<u8>>,
        disco: Option<Vec<u8>>,
        network_lock: Option<Vec<u8>>,
    ) -> Self {
        let mut out = Self {
            ..ts::keys::NodeState::default().into()
        };

        if let Some(machine) = machine {
            out.machine = machine;
        }

        if let Some(node) = node {
            out.node = node;
        }

        if let Some(disco) = disco {
            out.disco = disco;
        }

        if let Some(network_lock) = network_lock {
            out.network_lock = network_lock;
        }

        out
    }

    pub fn __repr__(&self) -> String {
        match tailscale::keys::NodeState::try_from(self) {
            Ok(state) => {
                format!(
                    "tailscale.Keystate(machine={}, node={}, disco={}, network_lock={})",
                    hex::encode(state.machine_keys.public.to_bytes()),
                    hex::encode(state.node_keys.public.to_bytes()),
                    hex::encode(state.disco_keys.public.to_bytes()),
                    hex::encode(state.network_lock_keys.public.to_bytes()),
                )
            }
            Err(_) => "tailscale.Keystate(<invalid>)".to_owned(),
        }
    }
}

impl From<tailscale::keys::NodeState> for Keystate {
    fn from(value: tailscale::keys::NodeState) -> Self {
        Self {
            machine: value.machine_keys.private.to_bytes().into(),
            node: value.node_keys.private.to_bytes().into(),
            disco: value.disco_keys.private.to_bytes().into(),
            network_lock: value.network_lock_keys.private.to_bytes().into(),
        }
    }
}

impl TryFrom<&Keystate> for tailscale::keys::NodeState {
    type Error = ();

    fn try_from(value: &Keystate) -> Result<Self, ()> {
        fn key<T>(v: &[u8]) -> Result<T, ()>
        where
            T: From<[u8; 32]>,
        {
            Ok(<[u8; 32]>::try_from(v).map_err(|_| ())?.into())
        }

        Ok(Self {
            machine_keys: key::<MachinePrivateKey>(&value.machine)?.into(),
            node_keys: key::<NodePrivateKey>(&value.node)?.into(),
            disco_keys: key::<DiscoPrivateKey>(&value.disco)?.into(),
            network_lock_keys: key::<NetworkLockPrivateKey>(&value.network_lock)?.into(),
        })
    }
}
