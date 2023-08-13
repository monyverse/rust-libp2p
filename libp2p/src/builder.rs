// TODO: Rename runtime to provider
// TODO: Should we have a timeout on transport?
// TODO: Be able to address `SwarmBuilder` configuration methods.

use libp2p_core::{muxing::StreamMuxerBox, Transport};
use std::marker::PhantomData;

pub struct SwarmBuilder {}

impl SwarmBuilder {
    pub fn new() -> SwarmBuilder {
        Self {}
    }

    pub fn with_new_identity(self) -> ProviderBuilder {
        self.with_existing_identity(libp2p_identity::Keypair::generate_ed25519())
    }

    pub fn with_existing_identity(self, keypair: libp2p_identity::Keypair) -> ProviderBuilder {
        ProviderBuilder { keypair }
    }
}

pub struct ProviderBuilder {
    keypair: libp2p_identity::Keypair,
}

impl ProviderBuilder {
    #[cfg(feature = "async-std")]
    pub fn with_async_std(self) -> TcpBuilder<AsyncStd> {
        TcpBuilder {
            keypair: self.keypair,
            phantom: PhantomData,
        }
    }

    #[cfg(feature = "tokio")]
    pub fn with_tokio(self) -> TcpBuilder<Tokio> {
        TcpBuilder {
            keypair: self.keypair,
            phantom: PhantomData,
        }
    }
}

pub struct TcpBuilder<P> {
    keypair: libp2p_identity::Keypair,
    phantom: PhantomData<P>,
}

#[cfg(feature = "tcp")]
impl<P> TcpBuilder<P> {
    pub fn with_tcp(self) -> TcpTlsBuilder<P> {
        self.with_tcp_config(Default::default())
    }

    pub fn with_tcp_config(self, config: libp2p_tcp::Config) -> TcpTlsBuilder<P> {
        TcpTlsBuilder {
            config,
            keypair: self.keypair,
            phantom: PhantomData,
        }
    }
}

impl<P> TcpBuilder<P> {
    // TODO: This would allow one to build a faulty transport.
    pub fn without_tcp(self) -> RelayBuilder<P, impl AuthenticatedMultiplexedTransport> {
        RelayBuilder {
            // TODO: Is this a good idea in a production environment? Unfortunately I don't know a
            // way around it. One can not define two `with_relay` methods, one with a real transport
            // using OrTransport, one with a fake transport discarding it right away.
            transport: libp2p_core::transport::dummy::DummyTransport::new(),
            keypair: self.keypair,
            phantom: PhantomData,
        }
    }
}

#[cfg(feature = "tcp")]
pub struct TcpTlsBuilder<P> {
    config: libp2p_tcp::Config,
    keypair: libp2p_identity::Keypair,
    phantom: PhantomData<P>,
}

#[cfg(feature = "tcp")]
impl<P> TcpTlsBuilder<P> {
    #[cfg(feature = "tls")]
    pub fn with_tls(self) -> TcpNoiseBuilder<P, Tls> {
        TcpNoiseBuilder {
            config: self.config,
            keypair: self.keypair,
            phantom: PhantomData,
        }
    }

    pub fn without_tls(self) -> TcpNoiseBuilder<P, WithoutTls> {
        TcpNoiseBuilder {
            config: self.config,
            keypair: self.keypair,
            phantom: PhantomData,
        }
    }
}

// Shortcuts
#[cfg(all(feature = "tcp", feature = "noise", feature = "async-std"))]
impl TcpTlsBuilder<AsyncStd> {
    #[cfg(feature = "noise")]
    pub fn with_noise(self) -> RelayBuilder<AsyncStd, impl AuthenticatedMultiplexedTransport> {
        self.without_tls().with_noise()
    }
}
#[cfg(all(feature = "tcp", feature = "noise", feature = "tokio"))]
impl TcpTlsBuilder<Tokio> {
    #[cfg(feature = "noise")]
    pub fn with_noise(self) -> RelayBuilder<Tokio, impl AuthenticatedMultiplexedTransport> {
        self.without_tls().with_noise()
    }
}

#[cfg(feature = "tcp")]
pub struct TcpNoiseBuilder<P, A> {
    config: libp2p_tcp::Config,
    keypair: libp2p_identity::Keypair,
    phantom: PhantomData<(P, A)>,
}

#[cfg(feature = "tcp")]
macro_rules! construct_relay_builder {
    ($self:ident, $tcp:ident, $auth:expr) => {
        RelayBuilder {
            transport: libp2p_tcp::$tcp::Transport::new($self.config)
                .upgrade(libp2p_core::upgrade::Version::V1Lazy)
                // TODO: Handle unwrap?
                .authenticate($auth)
                .multiplex(libp2p_yamux::Config::default())
                .map(|(p, c), _| (p, StreamMuxerBox::new(c))),
            keypair: $self.keypair,
            phantom: PhantomData,
        }
    };
}

macro_rules! impl_tcp_noise_builder {
    ($runtimeKebabCase:literal, $runtimeCamelCase:ident, $tcp:ident) => {
        #[cfg(all(feature = $runtimeKebabCase, feature = "tcp", feature = "tls"))]
        impl TcpNoiseBuilder<$runtimeCamelCase, Tls> {
            #[cfg(feature = "noise")]
            pub fn with_noise(
                self,
            ) -> RelayBuilder<$runtimeCamelCase, impl AuthenticatedMultiplexedTransport> {
                construct_relay_builder!(
                    self,
                    $tcp,
                    libp2p_core::upgrade::Map::new(
                        libp2p_core::upgrade::SelectUpgrade::new(
                            libp2p_tls::Config::new(&self.keypair).unwrap(),
                            libp2p_noise::Config::new(&self.keypair).unwrap(),
                        ),
                        |upgrade| match upgrade {
                            futures::future::Either::Left((peer_id, upgrade)) => {
                                (peer_id, futures::future::Either::Left(upgrade))
                            }
                            futures::future::Either::Right((peer_id, upgrade)) => {
                                (peer_id, futures::future::Either::Right(upgrade))
                            }
                        },
                    )
                )
            }

            pub fn without_noise(
                self,
            ) -> RelayBuilder<$runtimeCamelCase, impl AuthenticatedMultiplexedTransport> {
                construct_relay_builder!(
                    self,
                    $tcp,
                    libp2p_tls::Config::new(&self.keypair).unwrap()
                )
            }
        }

        #[cfg(feature = $runtimeKebabCase)]
        impl TcpNoiseBuilder<$runtimeCamelCase, WithoutTls> {
            #[cfg(feature = "noise")]
            pub fn with_noise(
                self,
            ) -> RelayBuilder<$runtimeCamelCase, impl AuthenticatedMultiplexedTransport> {
                construct_relay_builder!(
                    self,
                    $tcp,
                    libp2p_noise::Config::new(&self.keypair).unwrap()
                )
            }
        }
    };
}

impl_tcp_noise_builder!("async-std", AsyncStd, async_io);
impl_tcp_noise_builder!("tokio", Tokio, tokio);

pub enum Tls {}
pub enum WithoutTls {}

pub struct RelayBuilder<P, T> {
    transport: T,
    keypair: libp2p_identity::Keypair,
    phantom: PhantomData<P>,
}

// TODO: Noise feature
#[cfg(feature = "relay")]
impl<P, T> RelayBuilder<P, T> {
    // TODO: This should be with_relay_client.
    pub fn with_relay(self) -> RelayTlsBuilder<P, T> {
        RelayTlsBuilder {
            transport: self.transport,
            keypair: self.keypair,
            phantom: PhantomData,
        }
    }
}

pub struct NoRelayBehaviour;

impl<P, T> RelayBuilder<P, T> {
    pub fn without_relay(self) -> OtherTransportBuilder<P, T, NoRelayBehaviour> {
        OtherTransportBuilder {
            transport: self.transport,
            relay_behaviour: NoRelayBehaviour,
            keypair: self.keypair,
            phantom: PhantomData,
        }
    }
}

// Shortcuts
impl<P, T: AuthenticatedMultiplexedTransport> RelayBuilder<P, T> {
    pub fn with_other_transport<OtherTransport: AuthenticatedMultiplexedTransport>(
        self,
        constructor: impl FnMut(&libp2p_identity::Keypair) -> OtherTransport,
    ) -> OtherTransportBuilder<P, impl AuthenticatedMultiplexedTransport, NoRelayBehaviour> {
        self.without_relay().with_other_transport(constructor)
    }

    pub fn with_behaviour<B>(
        self,
        constructor: impl FnMut(&libp2p_identity::Keypair) -> B,
    ) -> Builder<P, B> {
        self.without_relay()
            .without_any_other_transports()
            .without_dns()
            .without_websocket()
            .with_behaviour(constructor)
    }
}

#[cfg(feature = "relay")]
pub struct RelayTlsBuilder<P, T> {
    transport: T,
    keypair: libp2p_identity::Keypair,
    phantom: PhantomData<P>,
}

#[cfg(feature = "relay")]
impl<P, T> RelayTlsBuilder<P, T> {
    #[cfg(feature = "tls")]
    pub fn with_tls(self) -> RelayNoiseBuilder<P, T, Tls> {
        RelayNoiseBuilder {
            transport: self.transport,
            keypair: self.keypair,
            phantom: PhantomData,
        }
    }

    pub fn without_tls(self) -> RelayNoiseBuilder<P, T, WithoutTls> {
        RelayNoiseBuilder {
            transport: self.transport,
            keypair: self.keypair,
            phantom: PhantomData,
        }
    }
}

// Shortcuts
#[cfg(all(feature = "relay", feature = "noise", feature = "async-std"))]
impl<T: AuthenticatedMultiplexedTransport> RelayTlsBuilder<AsyncStd, T> {
    #[cfg(feature = "noise")]
    pub fn with_noise(
        self,
    ) -> OtherTransportBuilder<
        AsyncStd,
        impl AuthenticatedMultiplexedTransport,
        libp2p_relay::client::Behaviour,
    > {
        self.without_tls().with_noise()
    }
}
#[cfg(all(feature = "relay", feature = "noise", feature = "tokio"))]
impl<T: AuthenticatedMultiplexedTransport> RelayTlsBuilder<Tokio, T> {
    #[cfg(feature = "noise")]
    pub fn with_noise(
        self,
    ) -> OtherTransportBuilder<
        Tokio,
        impl AuthenticatedMultiplexedTransport,
        libp2p_relay::client::Behaviour,
    > {
        self.without_tls().with_noise()
    }
}

#[cfg(feature = "relay")]
pub struct RelayNoiseBuilder<P, T, A> {
    transport: T,
    keypair: libp2p_identity::Keypair,
    phantom: PhantomData<(P, A)>,
}

#[cfg(feature = "relay")]
macro_rules! construct_other_transport_builder {
    ($self:ident, $auth:expr) => {{
        let (relay_transport, relay_behaviour) =
            libp2p_relay::client::new($self.keypair.public().to_peer_id());

        OtherTransportBuilder {
            transport: $self
                .transport
                .or_transport(
                    relay_transport
                        .upgrade(libp2p_core::upgrade::Version::V1Lazy)
                        .authenticate($auth)
                        .multiplex(libp2p_yamux::Config::default())
                        .map(|(p, c), _| (p, StreamMuxerBox::new(c))),
                )
                .map(|either, _| either.into_inner()),
            keypair: $self.keypair,
            relay_behaviour,
            phantom: PhantomData,
        }
    }};
}

#[cfg(all(feature = "relay", feature = "tls"))]
impl<P, T: AuthenticatedMultiplexedTransport> RelayNoiseBuilder<P, T, Tls> {
    #[cfg(feature = "noise")]
    pub fn with_noise(
        self,
    ) -> OtherTransportBuilder<
        P,
        impl AuthenticatedMultiplexedTransport,
        libp2p_relay::client::Behaviour,
    > {
        // TODO: Handle unwrap?
        construct_other_transport_builder!(
            self,
            libp2p_core::upgrade::Map::new(
                libp2p_core::upgrade::SelectUpgrade::new(
                    libp2p_tls::Config::new(&self.keypair).unwrap(),
                    libp2p_noise::Config::new(&self.keypair).unwrap(),
                ),
                |upgrade| match upgrade {
                    futures::future::Either::Left((peer_id, upgrade)) => {
                        (peer_id, futures::future::Either::Left(upgrade))
                    }
                    futures::future::Either::Right((peer_id, upgrade)) => {
                        (peer_id, futures::future::Either::Right(upgrade))
                    }
                },
            )
        )
    }
    pub fn without_noise(
        self,
    ) -> OtherTransportBuilder<
        P,
        impl AuthenticatedMultiplexedTransport,
        libp2p_relay::client::Behaviour,
    > {
        construct_other_transport_builder!(self, libp2p_tls::Config::new(&self.keypair).unwrap())
    }
}

#[cfg(feature = "relay")]
impl<P, T: AuthenticatedMultiplexedTransport> RelayNoiseBuilder<P, T, WithoutTls> {
    #[cfg(feature = "noise")]
    pub fn with_noise(
        self,
    ) -> OtherTransportBuilder<
        P,
        impl AuthenticatedMultiplexedTransport,
        libp2p_relay::client::Behaviour,
    > {
        construct_other_transport_builder!(self, libp2p_noise::Config::new(&self.keypair).unwrap())
    }
}

pub struct OtherTransportBuilder<P, T, R> {
    transport: T,
    relay_behaviour: R,
    keypair: libp2p_identity::Keypair,
    phantom: PhantomData<P>,
}

impl<P, T: AuthenticatedMultiplexedTransport, R> OtherTransportBuilder<P, T, R> {
    pub fn with_other_transport<OtherTransport: AuthenticatedMultiplexedTransport>(
        self,
        mut constructor: impl FnMut(&libp2p_identity::Keypair) -> OtherTransport,
    ) -> OtherTransportBuilder<P, impl AuthenticatedMultiplexedTransport, R> {
        OtherTransportBuilder {
            transport: self
                .transport
                .or_transport(constructor(&self.keypair))
                .map(|either, _| either.into_inner()),
            relay_behaviour: self.relay_behaviour,
            keypair: self.keypair,
            phantom: PhantomData,
        }
    }

    // TODO: Not the ideal name.
    pub fn without_any_other_transports(
        self,
    ) -> DnsBuilder<P, impl AuthenticatedMultiplexedTransport, R> {
        DnsBuilder {
            transport: self.transport,
            relay_behaviour: self.relay_behaviour,
            keypair: self.keypair,
            phantom: PhantomData,
        }
    }
}

pub struct DnsBuilder<P, T, R> {
    transport: T,
    relay_behaviour: R,
    keypair: libp2p_identity::Keypair,
    phantom: PhantomData<P>,
}

#[cfg(all(feature = "async-std", feature = "dns"))]
impl<T: AuthenticatedMultiplexedTransport, R> DnsBuilder<AsyncStd, T, R> {
    pub async fn with_dns(
        self,
    ) -> WebsocketBuilder<AsyncStd, impl AuthenticatedMultiplexedTransport, R> {
        WebsocketBuilder {
            keypair: self.keypair,
            relay_behaviour: self.relay_behaviour,
            // TODO: Timeout needed?
            transport: libp2p_dns::DnsConfig::system(self.transport)
                .await
                .expect("TODO: Handle"),
            phantom: PhantomData,
        }
    }
}

#[cfg(all(feature = "tokio", feature = "dns"))]
impl<T: AuthenticatedMultiplexedTransport, R> DnsBuilder<Tokio, T, R> {
    pub fn with_dns(self) -> WebsocketBuilder<Tokio, impl AuthenticatedMultiplexedTransport, R> {
        WebsocketBuilder {
            keypair: self.keypair,
            relay_behaviour: self.relay_behaviour,
            // TODO: Timeout needed?
            transport: libp2p_dns::TokioDnsConfig::system(self.transport).expect("TODO: Handle"),
            phantom: PhantomData,
        }
    }
}

impl<P, T, R> DnsBuilder<P, T, R> {
    pub fn without_dns(self) -> WebsocketBuilder<P, T, R> {
        WebsocketBuilder {
            keypair: self.keypair,
            relay_behaviour: self.relay_behaviour,
            // TODO: Timeout needed?
            transport: self.transport,
            phantom: PhantomData,
        }
    }
}

// Shortcuts
#[cfg(feature = "relay")]
impl<P, T: AuthenticatedMultiplexedTransport> DnsBuilder<P, T, libp2p_relay::client::Behaviour> {
    pub fn with_behaviour<B>(
        self,
        constructor: impl FnMut(&libp2p_identity::Keypair, libp2p_relay::client::Behaviour) -> B,
    ) -> Builder<P, B> {
        self.without_dns()
            .without_websocket()
            .with_behaviour(constructor)
    }
}
impl<P, T: AuthenticatedMultiplexedTransport> DnsBuilder<P, T, NoRelayBehaviour> {
    pub fn with_behaviour<B>(
        self,
        constructor: impl FnMut(&libp2p_identity::Keypair) -> B,
    ) -> Builder<P, B> {
        self.without_dns()
            .without_websocket()
            .with_behaviour(constructor)
    }
}

pub struct WebsocketBuilder<P, T, R> {
    transport: T,
    relay_behaviour: R,
    keypair: libp2p_identity::Keypair,
    phantom: PhantomData<P>,
}

#[cfg(feature = "websocket")]
impl<P, T, R> WebsocketBuilder<P, T, R> {
    pub fn with_websocket(self) -> WebsocketTlsBuilder<P, T, R> {
        WebsocketTlsBuilder {
            transport: self.transport,
            relay_behaviour: self.relay_behaviour,
            keypair: self.keypair,
            phantom: PhantomData,
        }
    }
}

impl<P, T: AuthenticatedMultiplexedTransport, R> WebsocketBuilder<P, T, R> {
    pub fn without_websocket(self) -> BehaviourBuilder<P, R> {
        BehaviourBuilder {
            keypair: self.keypair,
            relay_behaviour: self.relay_behaviour,
            // TODO: Timeout needed?
            transport: self.transport.boxed(),
            phantom: PhantomData,
        }
    }
}

#[cfg(feature = "websocket")]
pub struct WebsocketTlsBuilder<P, T, R> {
    transport: T,
    relay_behaviour: R,
    keypair: libp2p_identity::Keypair,
    phantom: PhantomData<P>,
}

#[cfg(feature = "websocket")]
impl<P, T, R> WebsocketTlsBuilder<P, T, R> {
    #[cfg(feature = "tls")]
    pub fn with_tls(self) -> WebsocketNoiseBuilder<P, T, R, Tls> {
        WebsocketNoiseBuilder {
            relay_behaviour: self.relay_behaviour,
            transport: self.transport,
            keypair: self.keypair,
            phantom: PhantomData,
        }
    }

    pub fn without_tls(self) -> WebsocketNoiseBuilder<P, T, R, WithoutTls> {
        WebsocketNoiseBuilder {
            relay_behaviour: self.relay_behaviour,
            transport: self.transport,
            keypair: self.keypair,
            phantom: PhantomData,
        }
    }
}

#[cfg(feature = "websocket")]
pub struct WebsocketNoiseBuilder<P, T, R, A> {
    transport: T,
    relay_behaviour: R,
    keypair: libp2p_identity::Keypair,
    phantom: PhantomData<(P, A)>,
}

#[cfg(feature = "websocket")]
macro_rules! construct_behaviour_builder {
    ($self:ident, $tcp:ident, $auth:expr) => {{
        let websocket_transport = libp2p_websocket::WsConfig::new(
            futures::executor::block_on(libp2p_dns::DnsConfig::system(
                libp2p_tcp::$tcp::Transport::new(libp2p_tcp::Config::default()),
            ))
            .unwrap(),
        )
        .upgrade(libp2p_core::upgrade::Version::V1)
        .authenticate($auth)
        .multiplex(libp2p_yamux::Config::default())
        .map(|(p, c), _| (p, StreamMuxerBox::new(c)));

        BehaviourBuilder {
            transport: websocket_transport
                .or_transport($self.transport)
                .map(|either, _| either.into_inner())
                .boxed(),
            keypair: $self.keypair,
            relay_behaviour: $self.relay_behaviour,
            phantom: PhantomData,
        }
    }};
}

macro_rules! impl_websocket_noise_builder {
    ($runtimeKebabCase:literal, $runtimeCamelCase:ident, $tcp:ident) => {
        #[cfg(all(
                                                                    feature = $runtimeKebabCase,
                                                                    feature = "websocket",
                                                                    feature = "dns",
                                                                    feature = "websocket",
                                                                    feature = "tls"
                                                                ))]
        impl<T: AuthenticatedMultiplexedTransport, R>
            WebsocketNoiseBuilder<$runtimeCamelCase, T, R, Tls>
        {
            #[cfg(feature = "noise")]
            pub fn with_noise(self) -> BehaviourBuilder<$runtimeCamelCase, R> {
                construct_behaviour_builder!(
                    self,
                    $tcp,
                    libp2p_core::upgrade::Map::new(
                        libp2p_core::upgrade::SelectUpgrade::new(
                            libp2p_tls::Config::new(&self.keypair).unwrap(),
                            libp2p_noise::Config::new(&self.keypair).unwrap(),
                        ),
                        |upgrade| match upgrade {
                            futures::future::Either::Left((peer_id, upgrade)) => {
                                (peer_id, futures::future::Either::Left(upgrade))
                            }
                            futures::future::Either::Right((peer_id, upgrade)) => {
                                (peer_id, futures::future::Either::Right(upgrade))
                            }
                        },
                    )
                )
            }
            pub fn without_noise(self) -> BehaviourBuilder<$runtimeCamelCase, R> {
                construct_behaviour_builder!(
                    self,
                    $tcp,
                    libp2p_tls::Config::new(&self.keypair).unwrap()
                )
            }
        }

        #[cfg(all(feature = "$runtimeKebabCase", feature = "dns", feature = "websocket"))]
        impl<T: AuthenticatedMultiplexedTransport, R>
            WebsocketNoiseBuilder<$runtimeCamelCase, T, R, WithoutTls>
        {
            #[cfg(feature = "noise")]
            pub fn with_noise(self) -> BehaviourBuilder<$runtimeCamelCase, R> {
                construct_behaviour_builder!(
                    self,
                    $tcp,
                    libp2p_noise::Config::new(&self.keypair).unwrap(),
                )
            }
        }
    };
}

impl_websocket_noise_builder!("async-std", AsyncStd, async_io);
impl_websocket_noise_builder!("tokio", Tokio, tokio);

pub struct BehaviourBuilder<P, R> {
    keypair: libp2p_identity::Keypair,
    relay_behaviour: R,
    transport: libp2p_core::transport::Boxed<(libp2p_identity::PeerId, StreamMuxerBox)>,
    phantom: PhantomData<P>,
}

#[cfg(feature = "relay")]
impl<P> BehaviourBuilder<P, libp2p_relay::client::Behaviour> {
    pub fn with_behaviour<B>(
        self,
        mut constructor: impl FnMut(&libp2p_identity::Keypair, libp2p_relay::client::Behaviour) -> B,
    ) -> Builder<P, B> {
        Builder {
            behaviour: constructor(&self.keypair, self.relay_behaviour),
            keypair: self.keypair,
            transport: self.transport,
            phantom: PhantomData,
        }
    }
}

impl<P> BehaviourBuilder<P, NoRelayBehaviour> {
    pub fn with_behaviour<B>(
        self,
        mut constructor: impl FnMut(&libp2p_identity::Keypair) -> B,
    ) -> Builder<P, B> {
        // Discard `NoRelayBehaviour`.
        let _ = self.relay_behaviour;

        Builder {
            behaviour: constructor(&self.keypair),
            keypair: self.keypair,
            transport: self.transport,
            phantom: PhantomData,
        }
    }
}

pub struct Builder<P, B> {
    keypair: libp2p_identity::Keypair,
    behaviour: B,
    transport: libp2p_core::transport::Boxed<(libp2p_identity::PeerId, StreamMuxerBox)>,
    phantom: PhantomData<P>,
}

#[cfg(feature = "async-std")]
impl<B: libp2p_swarm::NetworkBehaviour> Builder<AsyncStd, B> {
    pub fn build(self) -> libp2p_swarm::Swarm<B> {
        libp2p_swarm::SwarmBuilder::with_async_std_executor(
            self.transport,
            self.behaviour,
            self.keypair.public().to_peer_id(),
        )
        .build()
    }
}

#[cfg(feature = "tokio")]
impl<B: libp2p_swarm::NetworkBehaviour> Builder<Tokio, B> {
    pub fn build(self) -> libp2p_swarm::Swarm<B> {
        libp2p_swarm::SwarmBuilder::with_tokio_executor(
            self.transport,
            self.behaviour,
            self.keypair.public().to_peer_id(),
        )
        .build()
    }
}

#[cfg(feature = "async-std")]
pub enum AsyncStd {}

#[cfg(feature = "tokio")]
pub enum Tokio {}

pub trait AuthenticatedMultiplexedTransport:
    Transport<
        Error = Self::E,
        Dial = Self::D,
        ListenerUpgrade = Self::U,
        Output = (libp2p_identity::PeerId, StreamMuxerBox),
    > + Send
    + Unpin
    + 'static
{
    type E: Send + Sync + 'static;
    type D: Send;
    type U: Send;
}

impl<T> AuthenticatedMultiplexedTransport for T
where
    T: Transport<Output = (libp2p_identity::PeerId, StreamMuxerBox)> + Send + Unpin + 'static,
    <T as Transport>::Error: Send + Sync + 'static,
    <T as Transport>::Dial: Send,
    <T as Transport>::ListenerUpgrade: Send,
{
    type E = T::Error;
    type D = T::Dial;
    type U = T::ListenerUpgrade;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(all(feature = "tokio", feature = "tcp", feature = "tls", feature = "noise"))]
    fn tcp() {
        let _: libp2p_swarm::Swarm<libp2p_swarm::dummy::Behaviour> = SwarmBuilder::new()
            .with_new_identity()
            .with_tokio()
            .with_tcp()
            .with_tls()
            .with_noise()
            .without_relay()
            .without_any_other_transports()
            .without_dns()
            .without_websocket()
            .with_behaviour(|_| libp2p_swarm::dummy::Behaviour)
            .build();
    }

    #[test]
    #[cfg(all(
        feature = "tokio",
        feature = "tcp",
        feature = "tls",
        feature = "noise",
        feature = "relay"
    ))]
    fn tcp_relay() {
        #[derive(libp2p_swarm::NetworkBehaviour)]
        #[behaviour(prelude = "libp2p_swarm::derive_prelude")]
        struct Behaviour {
            dummy: libp2p_swarm::dummy::Behaviour,
            relay: libp2p_relay::client::Behaviour,
        }

        let _: libp2p_swarm::Swarm<Behaviour> = SwarmBuilder::new()
            .with_new_identity()
            .with_tokio()
            .with_tcp()
            .with_tls()
            .with_noise()
            .with_relay()
            .with_tls()
            .with_noise()
            .without_any_other_transports()
            .without_dns()
            .without_websocket()
            .with_behaviour(|_, relay| Behaviour {
                dummy: libp2p_swarm::dummy::Behaviour,
                relay,
            })
            .build();
    }

    #[test]
    #[cfg(all(
        feature = "tokio",
        feature = "tcp",
        feature = "tls",
        feature = "noise",
        feature = "dns"
    ))]
    fn tcp_dns() {
        let _: libp2p_swarm::Swarm<libp2p_swarm::dummy::Behaviour> = SwarmBuilder::new()
            .with_new_identity()
            .with_tokio()
            .with_tcp()
            .with_tls()
            .with_noise()
            .without_relay()
            .without_any_other_transports()
            .with_dns()
            .without_websocket()
            .with_behaviour(|_| libp2p_swarm::dummy::Behaviour)
            .build();
    }

    /// Showcases how to provide custom transports unknown to the libp2p crate, e.g. QUIC or WebRTC.
    #[test]
    #[cfg(all(feature = "tokio", feature = "tcp", feature = "tls", feature = "noise"))]
    fn tcp_other_transport_other_transport() {
        let _: libp2p_swarm::Swarm<libp2p_swarm::dummy::Behaviour> = SwarmBuilder::new()
            .with_new_identity()
            .with_tokio()
            .with_tcp()
            .with_tls()
            .with_noise()
            .without_relay()
            .with_other_transport(|_| libp2p_core::transport::dummy::DummyTransport::new())
            .with_other_transport(|_| libp2p_core::transport::dummy::DummyTransport::new())
            .with_other_transport(|_| libp2p_core::transport::dummy::DummyTransport::new())
            .without_any_other_transports()
            .without_dns()
            .without_websocket()
            .with_behaviour(|_| libp2p_swarm::dummy::Behaviour)
            .build();
    }

    #[test]
    #[cfg(all(
        feature = "tokio",
        feature = "tcp",
        feature = "tls",
        feature = "noise",
        feature = "dns",
        feature = "websocket",
    ))]
    fn tcp_websocket() {
        let _: libp2p_swarm::Swarm<libp2p_swarm::dummy::Behaviour> = SwarmBuilder::new()
            .with_new_identity()
            .with_tokio()
            .with_tcp()
            .with_tls()
            .with_noise()
            .without_relay()
            .without_any_other_transports()
            .without_dns()
            .with_websocket()
            .with_tls()
            .with_noise()
            .with_behaviour(|_| libp2p_swarm::dummy::Behaviour)
            .build();
    }
}