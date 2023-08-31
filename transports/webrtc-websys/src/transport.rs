use super::upgrade;
use super::Connection;
use super::Error;
use futures::future::FutureExt;
use libp2p_core::multiaddr::Multiaddr;
use libp2p_core::muxing::StreamMuxerBox;
use libp2p_core::transport::{Boxed, ListenerId, Transport as _, TransportError, TransportEvent};
use libp2p_identity::{Keypair, PeerId};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Config for the [`Transport`].
#[derive(Clone)]
pub struct Config {
    keypair: Keypair,
}

/// A WebTransport [`Transport`](libp2p_core::Transport) that works with `web-sys`.
pub struct Transport {
    config: Config,
}

impl Config {
    /// Constructs a new configuration for the [`Transport`].
    pub fn new(keypair: &Keypair) -> Self {
        Config {
            keypair: keypair.to_owned(),
        }
    }
}

impl Transport {
    /// Constructs a new `Transport` with the given [`Config`].
    pub fn new(config: Config) -> Transport {
        Transport { config }
    }

    /// Wraps `Transport` in [`Boxed`] and makes it ready to be consumed by
    /// SwarmBuilder.
    pub fn boxed(self) -> Boxed<(PeerId, StreamMuxerBox)> {
        self.map(|(peer_id, muxer), _| (peer_id, StreamMuxerBox::new(muxer)))
            .boxed()
    }
}

impl libp2p_core::Transport for Transport {
    type Output = (PeerId, Connection);
    type Error = Error;
    type ListenerUpgrade = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;
    type Dial = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn listen_on(
        &mut self,
        _id: ListenerId,
        addr: Multiaddr,
    ) -> Result<(), TransportError<Self::Error>> {
        Err(TransportError::MultiaddrNotSupported(addr))
    }

    fn remove_listener(&mut self, _id: ListenerId) -> bool {
        false
    }

    fn dial(&mut self, addr: Multiaddr) -> Result<Self::Dial, TransportError<Self::Error>> {
        let (sock_addr, server_fingerprint) = libp2p_webrtc_utils::parse_webrtc_dial_addr(&addr)
            .ok_or_else(|| TransportError::MultiaddrNotSupported(addr.clone()))?;

        if sock_addr.port() == 0 || sock_addr.ip().is_unspecified() {
            return Err(TransportError::MultiaddrNotSupported(addr));
        }

        let config = self.config.clone();

        Ok(async move {
            let (peer_id, connection) =
                upgrade::outbound(sock_addr, server_fingerprint, config.keypair.clone()).await?;

            Ok((peer_id, connection))
        }
        .boxed())
    }

    fn dial_as_listener(
        &mut self,
        addr: Multiaddr,
    ) -> Result<Self::Dial, TransportError<Self::Error>> {
        Err(TransportError::MultiaddrNotSupported(addr))
    }

    fn poll(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<TransportEvent<Self::ListenerUpgrade, Self::Error>> {
        Poll::Pending
    }

    fn address_translation(&self, _listen: &Multiaddr, _observed: &Multiaddr) -> Option<Multiaddr> {
        None
    }
}