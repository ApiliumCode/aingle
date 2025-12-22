use crate::*;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use ghost_actor::dependencies::tracing;
use rustls::pki_types::ServerName;
use std::convert::TryFrom;
use std::io::Read;
use std::io::Write;

pub(crate) fn spawn_tls_client(
    short: String,
    expected_proxy_url: ProxyUrl,
    tls_client_config: Arc<rustls::ClientConfig>,
    send: TransportChannelWrite,
    recv: TransportChannelRead,
    write: futures::channel::mpsc::Sender<ProxyWire>,
    read: futures::channel::mpsc::Receiver<ProxyWire>,
) -> tokio::sync::oneshot::Receiver<TransportResult<()>> {
    let (setup_send, setup_recv) = tokio::sync::oneshot::channel();
    metric_task(tls_client(
        short,
        setup_send,
        expected_proxy_url,
        tls_client_config,
        send,
        recv,
        write,
        read,
    ));
    setup_recv
}

#[allow(clippy::too_many_arguments)]
async fn tls_client(
    short: String,
    setup_send: tokio::sync::oneshot::Sender<TransportResult<()>>,
    expected_proxy_url: ProxyUrl,
    tls_client_config: Arc<rustls::ClientConfig>,
    mut send: TransportChannelWrite,
    recv: TransportChannelRead,
    mut write: futures::channel::mpsc::Sender<ProxyWire>,
    read: futures::channel::mpsc::Receiver<ProxyWire>,
) -> TransportResult<()> {
    let mut setup_send = Some(setup_send);
    let res: TransportResult<()> = async {
        let server_name = ServerName::try_from("stub.stub".to_string())
            .map_err(|_| TransportError::from("Invalid server name"))?;
        let mut cli = rustls::ClientConnection::new(tls_client_config, server_name)
            .map_err(TransportError::other)?;
        let mut buf = [0_u8; 4096];
        let mut in_pre = std::io::Cursor::new(Vec::new());

        let mut merge = kitsune_p2p_types::auto_stream_select(recv, read);
        use kitsune_p2p_types::AutoStreamSelect::*;

        let mut wants_write_close = false;
        let mut did_post_handshake_work = false;
        loop {
            if !did_post_handshake_work && !cli.is_handshaking() {
                did_post_handshake_work = true;

                let peer_certs = cli
                    .peer_certificates()
                    .ok_or_else(|| TransportError::from("tls_cli: No peer tls"))?;
                let first_cert = peer_certs
                    .first()
                    .ok_or_else(|| TransportError::from("tls_cli: No peer tls"))?;
                let cert_digest = blake2b_32(first_cert.as_ref());

                let remote_proxy_url =
                    ProxyUrl::new(expected_proxy_url.as_base().as_str(), cert_digest.into())?;
                if let Some(setup_send) = setup_send.take() {
                    if expected_proxy_url == remote_proxy_url {
                        tracing::info!("{}: CLI: CONNECTED TLS: {}", short, remote_proxy_url);
                        let _ = setup_send.send(Ok(()));
                    } else {
                        let msg = format!(
                            "expected remote {} != received {}",
                            expected_proxy_url, remote_proxy_url,
                        );
                        let _ = setup_send.send(Err(msg.clone().into()));
                        return Err(msg.into());
                    }
                }
            }

            if cli.wants_write() {
                let mut data = Vec::new();
                cli.write_tls(&mut data).map_err(TransportError::other)?;
                tracing::trace!("{}: CLI tls wants write {} bytes", short, data.len());
                write
                    .send(ProxyWire::chan_send(data.into()))
                    .await
                    .map_err(TransportError::other)?;
            }

            if wants_write_close && !cli.is_handshaking() {
                tracing::trace!("{}: CLI closing outgoing", short);
                write.close().await.map_err(TransportError::other)?;
            }

            match merge.next().await {
                Some(Left(Some(data))) => {
                    tracing::trace!("{}: CLI outgoing {} bytes", short, data.len());
                    // Write data in chunks, flushing TLS data between chunks
                    let mut offset = 0;
                    while offset < data.len() {
                        let written = cli
                            .writer()
                            .write(&data[offset..])
                            .map_err(TransportError::other)?;
                        if written == 0 {
                            // Buffer full, need to flush TLS data first
                            while cli.wants_write() {
                                let mut tls_data = Vec::new();
                                cli.write_tls(&mut tls_data)
                                    .map_err(TransportError::other)?;
                                if !tls_data.is_empty() {
                                    tracing::trace!(
                                        "{}: CLI tls flush {} bytes",
                                        short,
                                        tls_data.len()
                                    );
                                    write
                                        .send(ProxyWire::chan_send(tls_data.into()))
                                        .await
                                        .map_err(TransportError::other)?;
                                } else {
                                    break;
                                }
                            }
                            // If still can't write, something is wrong
                            if cli
                                .writer()
                                .write(&data[offset..offset + 1])
                                .map_err(TransportError::other)?
                                == 0
                            {
                                return Err(TransportError::other("TLS write buffer full"));
                            }
                            offset += 1;
                        } else {
                            offset += written;
                        }
                    }
                    // Flush remaining TLS data
                    while cli.wants_write() {
                        let mut tls_data = Vec::new();
                        cli.write_tls(&mut tls_data)
                            .map_err(TransportError::other)?;
                        if !tls_data.is_empty() {
                            tracing::trace!("{}: CLI tls flush {} bytes", short, tls_data.len());
                            write
                                .send(ProxyWire::chan_send(tls_data.into()))
                                .await
                                .map_err(TransportError::other)?;
                        } else {
                            break;
                        }
                    }
                }
                Some(Left(None)) => {
                    tracing::trace!("{}: CLI wants close outgoing", short);
                    wants_write_close = true;
                }
                Some(Right(Some(wire))) => match wire {
                    ProxyWire::ChanSend(data) => {
                        tracing::trace!(
                            "{}: CLI incoming encrypted {} bytes",
                            short,
                            data.channel_data.len()
                        );
                        in_pre.get_mut().clear();
                        in_pre.set_position(0);
                        in_pre.get_mut().extend_from_slice(&data.channel_data);
                        let in_buf_len = in_pre.get_ref().len();
                        loop {
                            if in_pre.position() >= in_buf_len as u64 {
                                break;
                            }
                            cli.read_tls(&mut in_pre).map_err(TransportError::other)?;
                            cli.process_new_packets().map_err(TransportError::other)?;
                            while let Ok(size) = cli.reader().read(&mut buf) {
                                tracing::trace!("{}: CLI incoming decrypted {} bytes", short, size);
                                if size == 0 {
                                    break;
                                }
                                send.send(buf[..size].to_vec()).await?;
                            }
                        }
                    }
                    _ => return Err(format!("invalid wire: {:?}", wire).into()),
                },
                Some(Right(None)) => {
                    send.close().await?;
                }
                None => return Ok(()),
            }
        }
    }
    .await;

    if let Err(e) = res {
        tracing::error!("{} CLI: ERROR: {:?}", short, e);
        let fail = ProxyWire::failure(format!("{:?}", e));
        if let Some(setup_send) = setup_send.take() {
            let _ = setup_send.send(Err(e));
        }
        let _ = write.send(fail).await.map_err(TransportError::other);
    }

    Ok(())
}
