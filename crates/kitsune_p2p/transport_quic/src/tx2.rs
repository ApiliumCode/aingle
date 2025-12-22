#![allow(clippy::new_ret_no_self)]
//! kitsune tx2 quic transport backend

use futures::future::{BoxFuture, FutureExt};
use futures::stream::{BoxStream, StreamExt};
use kitsune_p2p_types::config::*;
use kitsune_p2p_types::dependencies::{ghost_actor::dependencies::tracing, serde_json};
use kitsune_p2p_types::tls::*;
use kitsune_p2p_types::tx2::tx2_adapter::*;
use kitsune_p2p_types::tx2::tx2_utils::*;
use kitsune_p2p_types::tx2::*;
use kitsune_p2p_types::*;
use std::convert::TryInto;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// Configuration for QuicBackendAdapt
#[non_exhaustive]
#[derive(Default)]
pub struct QuicConfig {
    /// Tls config
    /// Default: None = ephemeral.
    pub tls: Option<TlsConfig>,

    /// Tuning Params
    /// Default: None = default.
    pub tuning_params: Option<KitsuneP2pTuningParams>,
}

impl QuicConfig {
    /// into inner contents with default application
    pub async fn split(self) -> KitsuneResult<(TlsConfig, KitsuneP2pTuningParams)> {
        let QuicConfig { tls, tuning_params } = self;

        let tls = match tls {
            None => TlsConfig::new_ephemeral().await?,
            Some(tls) => tls,
        };

        let tuning_params = tuning_params.unwrap_or_else(KitsuneP2pTuningParams::default);

        Ok((tls, tuning_params))
    }
}

/// Quic endpoint bind adapter for kitsune tx2
pub async fn tx2_quic_adapter(config: QuicConfig) -> KitsuneResult<AdapterFactory> {
    QuicBackendAdapt::new(config).await
}

// -- private -- //

/// Tls ALPN identifier for kitsune quic handshaking
const ALPN_KITSUNE_QUIC_0: &[u8] = b"kitsune-quic/0";

struct QuicInChanRecvAdapt(BoxStream<'static, InChanFut>);

impl QuicInChanRecvAdapt {
    pub fn new(con: quinn::Connection) -> Self {
        Self(
            futures::stream::unfold(con, move |con| async move {
                match con.accept_uni().await {
                    Err(_) => None,
                    Ok(recv) => Some((
                        async move {
                            // Use compat() to convert from tokio::io::AsyncRead to futures::AsyncRead
                            let in_: InChan = Box::new(FramedReader::new(Box::new(recv.compat())));
                            Ok(in_)
                        }
                        .boxed(),
                        con,
                    )),
                }
            })
            .boxed(),
        )
    }
}

impl futures::stream::Stream for QuicInChanRecvAdapt {
    type Item = InChanFut;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let inner = &mut self.0;
        tokio::pin!(inner);
        futures::stream::Stream::poll_next(inner, cx)
    }
}

impl InChanRecvAdapt for QuicInChanRecvAdapt {}

struct QuicConAdaptInner {
    peer_cert: Tx2Cert,
    con: quinn::Connection,
}

struct QuicConAdapt(Share<QuicConAdaptInner>, Uniq, Tx2Cert, Tx2ConDir);

pub(crate) fn blake2b_32(data: &[u8]) -> Vec<u8> {
    blake2b_simd::Params::new()
        .hash_length(32)
        .to_state()
        .update(data)
        .finalize()
        .as_bytes()
        .to_vec()
}

impl QuicConAdapt {
    pub fn new(con: quinn::Connection, dir: Tx2ConDir) -> KitsuneResult<Self> {
        use rustls::pki_types::CertificateDer;
        let peer_cert: Tx2Cert = match con.peer_identity() {
            None => return Err("invalid peer certificate".into()),
            Some(chain) => {
                let certs: Vec<CertificateDer<'static>> = *chain
                    .downcast::<Vec<CertificateDer<'static>>>()
                    .map_err(|_| KitsuneError::from("Failed to downcast peer identity"))?;
                match certs.first() {
                    None => return Err("invalid peer certificate".into()),
                    Some(cert) => blake2b_32(cert.as_ref()).into(),
                }
            }
        };
        Ok(Self(
            Share::new(QuicConAdaptInner {
                peer_cert: peer_cert.clone(),
                con,
            }),
            Uniq::default(),
            peer_cert,
            dir,
        ))
    }
}

impl ConAdapt for QuicConAdapt {
    fn uniq(&self) -> Uniq {
        self.1
    }

    fn dir(&self) -> Tx2ConDir {
        self.3
    }

    fn peer_addr(&self) -> KitsuneResult<TxUrl> {
        let addr = self.0.share_mut(|i, _| Ok(i.con.remote_address()))?;

        use kitsune_p2p_types::dependencies::url2;
        let url = url2::url2!("{}://{}", crate::SCHEME, addr);

        Ok(url.into())
    }

    fn peer_cert(&self) -> Tx2Cert {
        self.2.clone()
    }

    fn out_chan(&self, timeout: KitsuneTimeout) -> OutChanFut {
        let maybe_con = self.0.share_mut(|i, _| Ok(i.con.clone()));
        timeout
            .mix(async move {
                let con = maybe_con?;
                let out = con.open_uni().await.map_err(KitsuneError::other)?;
                // Use compat_write() to convert from tokio::io::AsyncWrite to futures::AsyncWrite
                let out: OutChan = Box::new(FramedWriter::new(Box::new(out.compat_write())));
                Ok(out)
            })
            .boxed()
    }

    fn is_closed(&self) -> bool {
        self.0.is_closed()
    }

    fn close(&self, code: u32, reason: &str) -> BoxFuture<'static, ()> {
        let _ = self.0.share_mut(|i, c| {
            tracing::info!(
                peer_cert=?i.peer_cert,
                %code,
                %reason,
                "close connection (quic)",
            );
            *c = true;
            i.con.close(code.into(), reason.as_bytes());
            Ok(())
        });
        async move {}.boxed()
    }
}

fn connecting(con_fut: quinn::Connecting, local_cert: Tx2Cert, dir: Tx2ConDir) -> ConFut {
    async move {
        let connection = con_fut.await.map_err(KitsuneError::other)?;

        let con: Arc<dyn ConAdapt> = Arc::new(QuicConAdapt::new(connection.clone(), dir)?);
        let chan_recv: Box<dyn InChanRecvAdapt> = Box::new(QuicInChanRecvAdapt::new(connection));

        let peer_cert = con.peer_cert();
        let url = con.peer_addr()?;
        match dir {
            Tx2ConDir::Outgoing => {
                tracing::info!(?local_cert, ?peer_cert, %url, "established outgoing connection (quic)");
            }
            Tx2ConDir::Incoming => {
                tracing::info!(?local_cert, ?peer_cert, %url, "established incoming connection (quic)");
            }
        }

        Ok((con, chan_recv))
    }
    .boxed()
}

struct QuicConRecvAdapt(BoxStream<'static, ConFut>);

impl QuicConRecvAdapt {
    pub fn new(endpoint: quinn::Endpoint, local_cert: Tx2Cert) -> Self {
        Self(
            futures::stream::unfold(
                (endpoint, local_cert),
                move |(endpoint, local_cert)| async move {
                    match endpoint.accept().await {
                        Some(incoming) => {
                            // In quinn 0.11, accept() returns Incoming which must be accepted
                            match incoming.accept() {
                                Ok(con) => Some((
                                    connecting(con, local_cert.clone(), Tx2ConDir::Incoming),
                                    (endpoint, local_cert),
                                )),
                                Err(_) => None,
                            }
                        }
                        None => None,
                    }
                },
            )
            .boxed(),
        )
    }
}

impl futures::stream::Stream for QuicConRecvAdapt {
    type Item = ConFut;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let inner = &mut self.0;
        tokio::pin!(inner);
        futures::stream::Stream::poll_next(inner, cx)
    }
}

impl ConRecvAdapt for QuicConRecvAdapt {}

struct QuicEndpointAdaptInner {
    ep: quinn::Endpoint,
    local_cert: Tx2Cert,
}

struct QuicEndpointAdapt(Share<QuicEndpointAdaptInner>, Uniq, Tx2Cert);

impl QuicEndpointAdapt {
    pub fn new(ep: quinn::Endpoint, local_cert: Tx2Cert) -> Self {
        Self(
            Share::new(QuicEndpointAdaptInner {
                ep,
                local_cert: local_cert.clone(),
            }),
            Uniq::default(),
            local_cert,
        )
    }
}

impl EndpointAdapt for QuicEndpointAdapt {
    fn debug(&self) -> serde_json::Value {
        match self.local_addr() {
            Ok(addr) => serde_json::json!({
                "type": "tx2_quic",
                "state": "open",
                "addr": addr,
            }),
            Err(_) => serde_json::json!({
                "type": "tx2_quic",
                "state": "closed",
            }),
        }
    }

    fn uniq(&self) -> Uniq {
        self.1
    }

    fn local_addr(&self) -> KitsuneResult<TxUrl> {
        let addr = self
            .0
            .share_mut(|i, _| i.ep.local_addr().map_err(KitsuneError::other))?;

        use kitsune_p2p_types::dependencies::url2;
        let mut url = url2::url2!("{}://{}", crate::SCHEME, addr);

        // TODO - FIXME - not sure how slow `get_if_addrs` is
        //                might be better to do this once on bind
        //                and just cache the bound address
        if let Some(host) = url.host_str() {
            if host == "0.0.0.0" {
                for iface in if_addrs::get_if_addrs().map_err(KitsuneError::other)? {
                    // super naive - just picking the first v4 that is not 127.0.0.1
                    let addr = iface.addr.ip();
                    if let std::net::IpAddr::V4(addr) = addr {
                        if addr != std::net::Ipv4Addr::from([127, 0, 0, 1]) {
                            url.set_host(Some(&iface.addr.ip().to_string())).unwrap();
                            break;
                        }
                    }
                }
            }
        }

        Ok(url.into())
    }

    fn local_cert(&self) -> Tx2Cert {
        self.2.clone()
    }

    fn connect(&self, url: TxUrl, timeout: KitsuneTimeout) -> ConFut {
        let maybe_ep = self
            .0
            .share_mut(|i, _| Ok((i.ep.clone(), i.local_cert.clone())));
        timeout
            .mix(async move {
                let (ep, local_cert) = maybe_ep?;
                let addr = crate::url_to_addr(url.as_url2(), crate::SCHEME)
                    .await
                    .map_err(KitsuneError::other)?;
                let con = ep.connect(addr, "stub.stub").map_err(KitsuneError::other);
                match connecting(con?, local_cert, Tx2ConDir::Outgoing).await {
                    Ok(con) => Ok(con),
                    Err(err) => {
                        tracing::warn!(?err, "failed to establish outgoing connection (quic)");
                        Err(err)
                    }
                }
            })
            .boxed()
    }

    fn is_closed(&self) -> bool {
        self.0.is_closed()
    }

    fn close(&self, code: u32, reason: &str) -> BoxFuture<'static, ()> {
        let _ = self.0.share_mut(|i, c| {
            tracing::warn!(
                local_cert=?i.local_cert,
                "CLOSING ENDPOINT"
            );
            *c = true;
            i.ep.close(code.into(), reason.as_bytes());
            Ok(())
        });
        async move {}.boxed()
    }
}

/// Quic endpoint backend bind adapter for kitsune tx2
pub struct QuicBackendAdapt {
    local_cert: Tx2Cert,
    tls_srv: Arc<quinn::crypto::rustls::QuicServerConfig>,
    tls_cli: Arc<quinn::crypto::rustls::QuicClientConfig>,
    transport: Arc<quinn::TransportConfig>,
}

impl QuicBackendAdapt {
    /// Construct a new quic tx2 backend bind adapter
    pub async fn new(config: QuicConfig) -> KitsuneResult<AdapterFactory> {
        use std::convert::TryFrom;
        let (tls, tuning_params) = config.split().await?;

        let local_cert = tls.cert_digest.clone().into();

        let (tls_srv, tls_cli) = gen_tls_configs(ALPN_KITSUNE_QUIC_0, &tls, tuning_params.clone())?;

        // Wrap rustls configs in quinn QUIC configs
        let tls_srv = Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(tls_srv)
                .map_err(KitsuneError::other)?,
        );
        let tls_cli = Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(tls_cli)
                .map_err(KitsuneError::other)?,
        );

        let mut transport = quinn::TransportConfig::default();

        // We don't use bidi streams in kitsune - only uni streams
        transport.max_concurrent_bidi_streams(0u8.into());

        // We don't use "Application" datagrams in kitsune -
        // only bidi streams.
        transport.datagram_receive_buffer_size(None);

        // see also `keep_alive_interval`.
        // right now keep_alive_interval is None,
        transport.max_idle_timeout(Some(
            Duration::from_millis(tuning_params.tx2_quic_max_idle_timeout_ms as u64)
                .try_into()
                .unwrap(),
        ));

        let transport = Arc::new(transport);

        tracing::debug!("build quinn configs");

        let out: AdapterFactory = Arc::new(Self {
            local_cert,
            tls_srv,
            tls_cli,
            transport,
        });

        Ok(out)
    }
}

impl BindAdapt for QuicBackendAdapt {
    fn bind(&self, url: TxUrl, timeout: KitsuneTimeout) -> EndpointFut {
        let local_cert = self.local_cert.clone();
        let tls_srv = self.tls_srv.clone();
        let tls_cli = self.tls_cli.clone();
        let transport = self.transport.clone();
        timeout
            .mix(async move {
                let addr = crate::url_to_addr(url.as_url2(), crate::SCHEME)
                    .await
                    .map_err(KitsuneError::other)?;

                // Build server config
                let mut server_config = quinn::ServerConfig::with_crypto(tls_srv);
                server_config.transport_config(transport.clone());

                // Build client config
                let mut client_config = quinn::ClientConfig::new(tls_cli);
                client_config.transport_config(transport);

                // Create endpoint
                let mut ep =
                    quinn::Endpoint::server(server_config, addr).map_err(KitsuneError::other)?;
                ep.set_default_client_config(client_config);

                let ep_clone = ep.clone();
                let ep: Arc<dyn EndpointAdapt> =
                    Arc::new(QuicEndpointAdapt::new(ep_clone.clone(), local_cert.clone()));
                let con_recv: Box<dyn ConRecvAdapt> =
                    Box::new(QuicConRecvAdapt::new(ep_clone, local_cert.clone()));

                let url = ep.local_addr()?;

                tracing::info!(?local_cert, %url, "bound local endpoint (quic)");

                Ok((ep, con_recv))
            })
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_quic_tx2() {
        kitsune_p2p_types::dependencies::observability::test_run().ok();

        let t = KitsuneTimeout::from_millis(5000);

        let (s_done, r_done) = tokio::sync::oneshot::channel();

        let config = QuicConfig::default();
        let factory = QuicBackendAdapt::new(config).await.unwrap();
        let (ep1, _con_recv1) = factory
            .bind("kitsune-quic://0.0.0.0:0".into(), t)
            .await
            .unwrap();

        let config = QuicConfig::default();
        let factory = QuicBackendAdapt::new(config).await.unwrap();
        let (ep2, mut con_recv2) = factory
            .bind("kitsune-quic://0.0.0.0:0".into(), t)
            .await
            .unwrap();

        let addr2 = ep2.local_addr().unwrap();
        println!("addr2: {}", addr2);

        let rt = kitsune_p2p_types::metrics::metric_task(async move {
            if let Some(mc) = con_recv2.next().await {
                let (_con, mut recv) = mc.await.unwrap();
                if let Some(mc) = recv.next().await {
                    let mut c = mc.await.unwrap();
                    let t = KitsuneTimeout::from_millis(5000);
                    let (_, data) = c.read(t).await.unwrap();
                    println!("GOT: {:?}", data.as_ref());
                    s_done.send(()).unwrap();
                }
            }
            KitsuneResult::Ok(())
        });

        let (c, _recv) = ep1.connect(addr2, t).await.unwrap();
        let mut c = c.out_chan(t).await.unwrap();

        let mut data = PoolBuf::new();
        data.extend_from_slice(b"hello");
        c.write(0.into(), data, t).await.unwrap();

        let debug = ep1.debug();
        println!("{}", serde_json::to_string_pretty(&debug).unwrap());

        r_done.await.unwrap();

        ep1.close(0, "").await;
        ep2.close(0, "").await;

        rt.await.unwrap().unwrap();
    }
}
