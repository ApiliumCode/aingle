use crate::*;
use futures::future::FutureExt;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use ghost_actor::dependencies::tracing;
use kitsune_p2p_types::dependencies::ghost_actor;
use kitsune_p2p_types::dependencies::ghost_actor::GhostControlSender;
use kitsune_p2p_types::dependencies::serde_json;
use kitsune_p2p_types::dependencies::url2;
use kitsune_p2p_types::transport::*;
use std::collections::HashMap;
use std::net::SocketAddr;

/// Convert quinn async read/write streams into Vec<u8> senders / receivers.
/// Quic bi-streams are Async Read/Write - But the kitsune transport api
/// uses Vec<u8> Streams / Sinks - This code translates into that.
fn tx_bi_chan(
    mut bi_send: quinn::SendStream,
    mut bi_recv: quinn::RecvStream,
) -> (TransportChannelWrite, TransportChannelRead) {
    let (write_send, mut write_recv) = futures::channel::mpsc::channel::<Vec<u8>>(10);
    let write_send = write_send.sink_map_err(TransportError::other);
    metric_task(async move {
        while let Some(data) = write_recv.next().await {
            bi_send
                .write_all(&data)
                .await
                .map_err(TransportError::other)?;
        }
        bi_send.finish().map_err(TransportError::other)?;
        TransportResult::Ok(())
    });
    let (mut read_send, read_recv) = futures::channel::mpsc::channel::<Vec<u8>>(10);
    metric_task(async move {
        let mut buf = [0_u8; 4096];
        loop {
            match bi_recv.read(&mut buf).await {
                Ok(Some(read)) => {
                    if read == 0 {
                        continue;
                    }
                    tracing::debug!("QUIC received {} bytes", read);
                    read_send
                        .send(buf[0..read].to_vec())
                        .await
                        .map_err(TransportError::other)?;
                }
                Ok(None) => break,
                Err(e) => return Err(TransportError::other(e)),
            }
        }
        TransportResult::Ok(())
    });
    let write_send: TransportChannelWrite = Box::new(write_send);
    let read_recv: TransportChannelRead = Box::new(read_recv);
    (write_send, read_recv)
}

/// QUIC implementation of kitsune TransportListener actor.
struct TransportListenerQuic {
    /// internal api logic
    internal_sender: ghost_actor::GhostSender<ListenerInner>,
    /// incoming channel send to our owner
    incoming_channel_sender: TransportEventSender,
    /// the url to return on 'bound_url' calls - what we bound to
    bound_url: Url2,
    /// the quinn binding (akin to a socket listener)
    quinn_endpoint: quinn::Endpoint,
    /// pool of active connections
    connections: HashMap<Url2, quinn::Connection>,
}

impl ghost_actor::GhostControlHandler for TransportListenerQuic {
    fn handle_ghost_actor_shutdown(
        mut self,
    ) -> ghost_actor::dependencies::must_future::MustBoxFuture<'static, ()> {
        async move {
            // Note: it's easiest to just blanket shut everything down.
            // If we wanted to be more graceful, we'd need to plumb
            // in some signals to start rejecting incoming connections,
            // then we could use `quinn_endpoint.wait_idle().await`.
            self.incoming_channel_sender.close_channel();
            for (_, con) in self.connections.into_iter() {
                con.close(0_u8.into(), b"");
                drop(con);
            }
            self.quinn_endpoint.close(0_u8.into(), b"");
        }
        .boxed()
        .into()
    }
}

ghost_actor::ghost_chan! {
    /// Internal Sender
    chan ListenerInner<TransportError> {
        /// Use our binding to establish a new outgoing connection.
        fn raw_connect(addr: SocketAddr) -> quinn::Connecting;

        /// Take a quinn connecting instance pulling it into our logic.
        /// Shared code for both incoming and outgoing connections.
        /// For outgoing create_channel we may also wish to create a channel.
        fn take_connecting(
            maybe_con: quinn::Connecting,
            with_channel: bool,
        ) -> Option<(
            Url2,
            TransportChannelWrite,
            TransportChannelRead,
        )>;

        /// Finalization step for taking control of a connection.
        /// Places it in our hash map for use establishing outgoing channels.
        fn set_connection(
            url: Url2,
            con: quinn::Connection,
        ) -> ();

        /// If we get an error making outgoing channels,
        /// or if the incoming channel receiver stops,
        /// we want to remove this connection from our pool. It is done.
        fn drop_connection(url: Url2) -> ();
    }
}

impl ghost_actor::GhostHandler<ListenerInner> for TransportListenerQuic {}

impl ListenerInnerHandler for TransportListenerQuic {
    fn handle_raw_connect(
        &mut self,
        addr: SocketAddr,
    ) -> ListenerInnerHandlerResult<quinn::Connecting> {
        tracing::debug!("attempt raw connect: {:?}", addr);
        let out = self
            .quinn_endpoint
            .connect(addr, "stub.stub")
            .map_err(TransportError::other)?;
        Ok(async move { Ok(out) }.boxed().into())
    }

    fn handle_take_connecting(
        &mut self,
        maybe_con: quinn::Connecting,
        with_channel: bool,
    ) -> ListenerInnerHandlerResult<Option<(Url2, TransportChannelWrite, TransportChannelRead)>>
    {
        let i_s = self.internal_sender.clone();
        let mut incoming_channel_sender = self.incoming_channel_sender.clone();
        Ok(async move {
            // Get the connection from Connecting
            let con = maybe_con.await.map_err(TransportError::other)?;

            // if we are making an outgoing connection
            // we also need to make an initial channel
            let out = if with_channel {
                let (bi_send, bi_recv) = con.open_bi().await.map_err(TransportError::other)?;
                Some(tx_bi_chan(bi_send, bi_recv))
            } else {
                None
            };

            // Construct our url from the low-level data
            let url = url2!("{}://{}", crate::SCHEME, con.remote_address());
            tracing::debug!("QUIC handle connection: {}", url);

            // Clone connection for the bi_streams task
            let con_clone = con.clone();

            // pass the connection off to our actor
            i_s.set_connection(url.clone(), con).await?;

            // pass any incoming channels off to our actor
            let url_clone = url.clone();
            metric_task(async move {
                loop {
                    match con_clone.accept_bi().await {
                        Ok((bi_send, bi_recv)) => {
                            let (write, read) = tx_bi_chan(bi_send, bi_recv);
                            if incoming_channel_sender
                                .send(TransportEvent::IncomingChannel(
                                    url_clone.clone(),
                                    write,
                                    read,
                                ))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                <Result<(), ()>>::Ok(())
            });

            Ok(out.map(move |(write, read)| (url, write, read)))
        }
        .boxed()
        .into())
    }

    fn handle_set_connection(
        &mut self,
        url: Url2,
        con: quinn::Connection,
    ) -> ListenerInnerHandlerResult<()> {
        self.connections.insert(url, con);
        Ok(async move { Ok(()) }.boxed().into())
    }

    fn handle_drop_connection(&mut self, url: Url2) -> ListenerInnerHandlerResult<()> {
        self.connections.remove(&url);
        Ok(async move { Ok(()) }.boxed().into())
    }
}

impl ghost_actor::GhostHandler<TransportListener> for TransportListenerQuic {}

impl TransportListenerHandler for TransportListenerQuic {
    fn handle_debug(&mut self) -> TransportListenerHandlerResult<serde_json::Value> {
        let url = self.bound_url.clone();
        let connections = self.connections.keys().cloned().collect::<Vec<_>>();
        Ok(async move {
            Ok(serde_json::json! {{
                "url": url,
                "connection_count": connections.len(),
            }})
        }
        .boxed()
        .into())
    }

    fn handle_bound_url(&mut self) -> TransportListenerHandlerResult<Url2> {
        let out = self.bound_url.clone();
        Ok(async move { Ok(out) }.boxed().into())
    }

    fn handle_create_channel(
        &mut self,
        url: Url2,
    ) -> TransportListenerHandlerResult<(Url2, TransportChannelWrite, TransportChannelRead)> {
        // Clone the connection if we have one to avoid lifetime issues
        let maybe_con = self.connections.get(&url).cloned();
        let url_clone = url.clone();

        let i_s = self.internal_sender.clone();
        Ok(async move {
            // if we already had a connection and the bi-stream
            // channel is successfully opened, return early using that
            if let Some(con) = maybe_con {
                match con.open_bi().await {
                    Ok((bi_send, bi_recv)) => {
                        let (write, read) = tx_bi_chan(bi_send, bi_recv);
                        return Ok((url_clone, write, read));
                    }
                    Err(_) => {
                        // otherwise, we should drop any existing channel
                        // we have... it no longer works for us
                        i_s.drop_connection(url_clone.clone()).await?;
                    }
                }
            }

            // we did not successfully use an existing connection.
            // instead, try establishing a new one with a new channel.
            let addr = crate::url_to_addr(&url_clone, crate::SCHEME).await?;
            let maybe_con = i_s.raw_connect(addr).await?;
            let (url, write, read) = i_s.take_connecting(maybe_con, true).await?.unwrap();

            Ok((url, write, read))
        }
        .boxed()
        .into())
    }
}

/// Spawn a new QUIC TransportListenerSender.
pub async fn spawn_transport_listener_quic(
    config: ConfigListenerQuic,
) -> TransportListenerResult<(
    ghost_actor::GhostSender<TransportListener>,
    TransportEventReceiver,
)> {
    let bind_to = config
        .bind_to
        .unwrap_or_else(|| url2::url2!("kitsune-quic://0.0.0.0:0"));
    let (server_config, client_config) = danger::configure_tls(config.tls)
        .await
        .map_err(|e| TransportError::from(format!("cert error: {:?}", e)))?;

    let addr = crate::url_to_addr(&bind_to, crate::SCHEME).await?;
    let mut quinn_endpoint =
        quinn::Endpoint::server(server_config, addr).map_err(TransportError::other)?;
    quinn_endpoint.set_default_client_config(client_config);

    let (incoming_channel_sender, receiver) = futures::channel::mpsc::channel(10);

    let builder = ghost_actor::actor_builder::GhostActorBuilder::new();

    let internal_sender = builder.channel_factory().create_channel().await?;

    let sender = builder.channel_factory().create_channel().await?;

    let i_s = internal_sender.clone();
    let endpoint_clone = quinn_endpoint.clone();
    metric_task(async move {
        while let Some(incoming) = endpoint_clone.accept().await {
            let i_s = i_s.clone();
            let res: TransportResult<()> = async {
                // In quinn 0.11, accept() returns Incoming, which needs to be accepted
                let connecting = incoming.accept().map_err(TransportError::other)?;
                i_s.take_connecting(connecting, false).await?;
                Ok(())
            }
            .await;
            if let Err(err) = res {
                ghost_actor::dependencies::tracing::error!(?err);
            }
        }

        // Our incoming connections ended,
        // this also indicates we cannot establish outgoing connections.
        // I.e., we need to shut down.
        i_s.ghost_actor_shutdown().await?;

        TransportResult::Ok(())
    });

    let mut bound_url = url2!(
        "{}://{}",
        crate::SCHEME,
        quinn_endpoint.local_addr().map_err(TransportError::other)?,
    );
    if let Some(override_host) = &config.override_host {
        bound_url.set_host(Some(override_host)).unwrap();
    } else if let Some(host) = bound_url.host_str() {
        if host == "0.0.0.0" {
            for iface in if_addrs::get_if_addrs().map_err(TransportError::other)? {
                // super naive - just picking the first v4 that is not 127.0.0.1
                let addr = iface.addr.ip();
                if let std::net::IpAddr::V4(addr) = addr {
                    if addr != std::net::Ipv4Addr::from([127, 0, 0, 1]) {
                        bound_url
                            .set_host(Some(&iface.addr.ip().to_string()))
                            .unwrap();
                        break;
                    }
                }
            }
        }
    }

    let actor = TransportListenerQuic {
        internal_sender,
        incoming_channel_sender,
        bound_url,
        quinn_endpoint,
        connections: HashMap::new(),
    };

    metric_task(builder.spawn(actor));

    Ok((sender, receiver))
}

mod danger {
    use kitsune_p2p_types::transport::TransportError;
    use kitsune_p2p_types::transport::TransportResult;
    use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
    use std::convert::{TryFrom, TryInto};
    use std::sync::Arc;
    use std::time::Duration;

    /// Configure TLS for both server and client using rustls 0.23 + quinn 0.11
    pub(crate) async fn configure_tls(
        cert: Option<(
            lair_keystore_api::actor::Cert,
            lair_keystore_api::actor::CertPrivKey,
        )>,
    ) -> TransportResult<(quinn::ServerConfig, quinn::ClientConfig)> {
        let (cert_der, cert_priv_der) = match cert {
            Some((c, p)) => (c.0.to_vec(), p.0.to_vec()),
            None => {
                let mut options = lair_keystore_api::actor::TlsCertOptions::default();
                options.alg = lair_keystore_api::actor::TlsCertAlg::PkcsEcdsaP256Sha256;
                let cert = lair_keystore_api::internal::tls::tls_cert_self_signed_new_from_entropy(
                    options,
                )
                .await
                .map_err(TransportError::other)?;
                (cert.cert_der.0.to_vec(), cert.priv_key_der.0.to_vec())
            }
        };

        let cert = CertificateDer::from(cert_der);
        let cert_priv = PrivatePkcs8KeyDer::from(cert_priv_der);

        // Server config with custom client verifier
        let server_crypto = rustls::ServerConfig::builder()
            .with_client_cert_verifier(Arc::new(SkipClientVerification))
            .with_single_cert(vec![cert.clone()], cert_priv.clone_key().into())
            .map_err(TransportError::other)?;

        let server_quic_config = quinn::crypto::rustls::QuicServerConfig::try_from(Arc::new(server_crypto))
            .map_err(TransportError::other)?;
        let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(server_quic_config));

        // Configure transport
        let mut transport = quinn::TransportConfig::default();
        transport.max_concurrent_uni_streams(0u8.into());
        transport.datagram_receive_buffer_size(None);
        transport.max_idle_timeout(Some(Duration::from_millis(30_000).try_into().unwrap()));
        let transport = Arc::new(transport);
        server_config.transport_config(transport.clone());

        // Client config with custom server verifier
        let client_crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
            .with_client_auth_cert(vec![cert], cert_priv.into())
            .map_err(TransportError::other)?;

        let client_quic_config = quinn::crypto::rustls::QuicClientConfig::try_from(Arc::new(client_crypto))
            .map_err(TransportError::other)?;
        let mut client_config = quinn::ClientConfig::new(Arc::new(client_quic_config));
        client_config.transport_config(transport);

        Ok((server_config, client_config))
    }

    /// Dummy client certificate verifier that accepts any client.
    #[derive(Debug)]
    struct SkipClientVerification;

    impl rustls::server::danger::ClientCertVerifier for SkipClientVerification {
        fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
            &[]
        }

        fn verify_client_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _now: UnixTime,
        ) -> Result<rustls::server::danger::ClientCertVerified, rustls::Error> {
            Ok(rustls::server::danger::ClientCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            vec![
                rustls::SignatureScheme::RSA_PKCS1_SHA256,
                rustls::SignatureScheme::RSA_PKCS1_SHA384,
                rustls::SignatureScheme::RSA_PKCS1_SHA512,
                rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
                rustls::SignatureScheme::RSA_PSS_SHA256,
                rustls::SignatureScheme::RSA_PSS_SHA384,
                rustls::SignatureScheme::RSA_PSS_SHA512,
                rustls::SignatureScheme::ED25519,
            ]
        }

        fn client_auth_mandatory(&self) -> bool {
            false
        }
    }

    /// Dummy server certificate verifier that treats any certificate as valid.
    /// NOTE, such verification is vulnerable to MITM attacks, but convenient for testing.
    #[derive(Debug)]
    struct SkipServerVerification;

    impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            vec![
                rustls::SignatureScheme::RSA_PKCS1_SHA256,
                rustls::SignatureScheme::RSA_PKCS1_SHA384,
                rustls::SignatureScheme::RSA_PKCS1_SHA512,
                rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
                rustls::SignatureScheme::RSA_PSS_SHA256,
                rustls::SignatureScheme::RSA_PSS_SHA384,
                rustls::SignatureScheme::RSA_PSS_SHA512,
                rustls::SignatureScheme::ED25519,
            ]
        }
    }
}
