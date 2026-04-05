#![allow(missing_docs)]

use core::{net::SocketAddr, time::Duration};

use clap::Parser;

#[derive(clap::Parser)]
#[command(version, about)]
struct Args {
    #[clap(flatten)]
    common: ts_cli_util::CommonArgs,

    /// Peer to send pings to.
    #[clap(short, long)]
    peer: SocketAddr,

    #[clap(short = 'i', long, default_value_t = 1.0)]
    ping_interval_secs: f64,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn core::error::Error>> {
    ts_cli_util::init_tracing();

    let args = Args::parse();
    let config = args.common.load_or_init_config().await?;

    let dev = tailscale::Device::new(
        config.control_config(),
        args.common.auth_key,
        config.key_state,
    )
    .await?;

    let sock = dev.udp_bind((dev.ipv4().await?, 1234).into()).await?;
    let mut ticker = tokio::time::interval(Duration::from_secs_f64(args.ping_interval_secs));

    loop {
        sock.send_to(args.peer, b"hello").await?;
        ticker.tick().await;
    }
}
