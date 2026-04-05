//! Ergonomic sockets API layer built around [`ts_netstack_smoltcp_core`].
//!
//! The idea is to take the command [`Channel`][ts_netstack_smoltcp_core::Channel] and use
//! it to expose an API that looks something like `std::net` or `tokio::net`, i.e. with
//! `UdpSocket`, `TcpStream`, `TcpListener`, etc. This is implemented here by internalizing
//! the `Channel` into each socket type so that they can all send commands on their own
//! behalf.
//!
//! Socket creation is implemented by [`CreateSocket`] as a set of extension methods on
//! `T where T:` [`HasChannel`][ts_netstack_smoltcp_core::HasChannel].
//!
//! ## Example
//!
//! Compare the example in [`ts_netstack_smoltcp_core`]:
//!
//! ```rust
//! # use core::net::SocketAddr;
//! # use bytes::Bytes;
//! # use netcore::smoltcp::time::Instant;
//! # use netcore::smoltcp::phy::Medium;
//! # use netcore::{Response, udp, HasChannel};
//! # use netsock::CreateSocket;
//! extern crate ts_netstack_smoltcp_core as netcore;
//! # extern crate ts_netstack_smoltcp_socket as netsock;
//!
//! // Construct a new netstack:
//! let mut stack = netcore::Netstack::new(netcore::Config::default(), Instant::ZERO);
//!
//! // Grab a channel through which we can send commands:
//! let channel = stack.command_channel();
//!
//! // Process the upcoming bind and send commands in the background (request() blocks
//! // for a response, hence the thread)
//! let thread = std::thread::spawn(move || {
//!     for i in 0..2 {
//!         let cmd = stack.wait_for_cmd_blocking(None).unwrap();
//!         stack.process_one_cmd(cmd);
//!     }
//!
//!     stack
//! });
//!
//! // Send a command to bind a UDP socket:
//! let sock = channel.udp_bind_blocking(([127, 0, 0, 1], 1000).into()).unwrap();
//! println!("bound udp socket: {sock:?}");
//!
//! sock.send_to_blocking(([1, 2, 3, 4], 80).into(), b"hello");
//! println!("sent udp packet");
//!
//! // Wait for the thread started above to finish processing the two UDP port commands:
//! let mut stack = thread.join().unwrap();
//!
//! // Pump the netstack to produce the IP packet that needs to be sent out on the network:
//! let (end1, end2) = netcore::Pipe::unbounded();
//! stack.poll_device_io(Instant::ZERO, &mut netcore::PipeDev {
//!     pipe: end1,
//!     medium: Medium::Ip,
//!     mtu: 1500,
//! });
//!
//! // Receive the packet from the pipe device:
//! let packet = end2.rx.recv().unwrap();
//! println!("packet: {packet:?}");
//!
//! // Sanity-check that the packet we got back is shaped correctly:
//! assert_eq!(packet.len(), netcore::smoltcp::wire::IPV4_HEADER_LEN + netcore::smoltcp::wire::UDP_HEADER_LEN + b"hello".len());
//! assert_eq!(packet[0] >> 4, 4); // ipv4 packet
//! assert!(packet.ends_with(b"hello"));
//! ```

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub extern crate ts_netstack_smoltcp_core as netcore;

#[doc(inline)]
pub use netcore::smoltcp::wire::IpProtocol;

/// Provide internal `request` and `request_async` helper methods for sockets, wrapping
/// [`netcore::request_blocking`] and [`netcore::request`]. Automatically passes
/// the internal socket `handle` and wraps/unwraps request and response types.
macro_rules! socket_requestor_impl {
    () => {
        fn request_blocking(
            &self,
            command: impl Into<$crate::netcore::Command>,
        ) -> Result<$crate::netcore::Response, $crate::netcore::Error> {
            ::netcore::HasChannel::request_blocking(&self.sender, Some(self.handle), command)
        }

        async fn request(
            &self,
            command: impl Into<$crate::netcore::Command>,
        ) -> Result<$crate::netcore::Response, $crate::netcore::Error> {
            ::netcore::HasChannel::request(&self.sender, Some(self.handle), command).await
        }
    };
}

mod create_socket;
pub use create_socket::CreateSocket;

mod raw;
mod tcp;
mod udp;

pub use raw::RawSocket;
pub use tcp::{TcpListener, TcpStream};
pub use udp::UdpSocket;
