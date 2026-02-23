use crate::actor::*;
use crate::AIngleP2pCell;
use crate::*;
use ::ai_fixt::prelude::*;
use ai_hash::fixt::AgentPubKeyFixturator;
use ai_hash::fixt::SafHashFixturator;
use ai_hash::AgentPubKey;
use ai_hash::SafHash;

struct StubNetwork;

impl ghost_actor::GhostHandler<AIngleP2p> for StubNetwork {}
impl ghost_actor::GhostControlHandler for StubNetwork {}

#[allow(unused_variables)]
impl AIngleP2pHandler for StubNetwork {
    fn handle_join(
        &mut self,
        saf_hash: SafHash,
        agent_pub_key: AgentPubKey,
    ) -> AIngleP2pHandlerResult<()> {
        Err("stub".into())
    }
    fn handle_leave(
        &mut self,
        saf_hash: SafHash,
        agent_pub_key: AgentPubKey,
    ) -> AIngleP2pHandlerResult<()> {
        Err("stub".into())
    }
    fn handle_call_remote(
        &mut self,
        saf_hash: SafHash,
        from_agent: AgentPubKey,
        to_agent: AgentPubKey,
        zome_name: ZomeName,
        fn_name: FunctionName,
        cap: Option<CapSecret>,
        payload: ExternIO,
    ) -> AIngleP2pHandlerResult<SerializedBytes> {
        Err("stub".into())
    }
    fn handle_publish(
        &mut self,
        saf_hash: SafHash,
        from_agent: AgentPubKey,
        request_validation_receipt: bool,
        sgd_hash: ai_hash::AnySgdHash,
        ops: Vec<(ai_hash::SgdOpHash, aingle_types::sgd_op::SgdOp)>,
        timeout_ms: Option<u64>,
    ) -> AIngleP2pHandlerResult<()> {
        Err("stub".into())
    }
    fn handle_get_validation_package(
        &mut self,
        input: actor::GetValidationPackage,
    ) -> AIngleP2pHandlerResult<ValidationPackageResponse> {
        Err("stub".into())
    }
    fn handle_get(
        &mut self,
        saf_hash: SafHash,
        from_agent: AgentPubKey,
        sgd_hash: ai_hash::AnySgdHash,
        options: actor::GetOptions,
    ) -> AIngleP2pHandlerResult<Vec<WireOps>> {
        Err("stub".into())
    }
    fn handle_get_meta(
        &mut self,
        saf_hash: SafHash,
        from_agent: AgentPubKey,
        sgd_hash: ai_hash::AnySgdHash,
        options: actor::GetMetaOptions,
    ) -> AIngleP2pHandlerResult<Vec<MetadataSet>> {
        Err("stub".into())
    }
    fn handle_get_links(
        &mut self,
        saf_hash: SafHash,
        from_agent: AgentPubKey,
        link_key: WireLinkKey,
        options: actor::GetLinksOptions,
    ) -> AIngleP2pHandlerResult<Vec<WireLinkOps>> {
        Err("stub".into())
    }
    fn handle_get_agent_activity(
        &mut self,
        saf_hash: SafHash,
        from_agent: AgentPubKey,
        agent: AgentPubKey,
        query: ChainQueryFilter,
        options: actor::GetActivityOptions,
    ) -> AIngleP2pHandlerResult<Vec<AgentActivityResponse<HeaderHash>>> {
        Err("stub".into())
    }
    fn handle_send_validation_receipt(
        &mut self,
        saf_hash: SafHash,
        to_agent: AgentPubKey,
        from_agent: AgentPubKey,
        receipt: SerializedBytes,
    ) -> AIngleP2pHandlerResult<()> {
        Err("stub".into())
    }
}

/// Spawn a stub network that doesn't respond to any messages.
/// Use `test_network()` if you want a real test network.
pub async fn stub_network() -> ghost_actor::GhostSender<AIngleP2p> {
    let builder = ghost_actor::actor_builder::GhostActorBuilder::new();

    let channel_factory = builder.channel_factory().clone();

    let sender = channel_factory.create_channel::<AIngleP2p>().await.unwrap();

    tokio::task::spawn(builder.spawn(StubNetwork));

    sender
}

fixturator!(
    AIngleP2pCell;
    curve Empty {
        tokio_helper::block_forever_on(async {
            let aingle_p2p = crate::test::stub_network().await;
            aingle_p2p.to_cell(
                SafHashFixturator::new(Empty).next().unwrap(),
                AgentPubKeyFixturator::new(Empty).next().unwrap(),
            )
        })
    };
    curve Unpredictable {
        AingleP2pCellFixturator::new(Empty).next().unwrap()
    };
    curve Predictable {
        AingleP2pCellFixturator::new(Empty).next().unwrap()
    };
);

#[cfg(test)]
mod tests {
    use crate::*;
    use ::ai_fixt::prelude::*;
    use futures::future::FutureExt;
    use ghost_actor::GhostControlSender;

    use aingle_zome_types::ValidationStatus;
    use kitsune_p2p::dependencies::kitsune_p2p_proxy::TlsConfig;
    use kitsune_p2p::KitsuneP2pConfig;

    macro_rules! newhash {
        ($p:ident, $c:expr) => {
            ai_hash::$p::from_raw_36([$c as u8; AI_HASH_UNTYPED_LEN].to_vec())
        };
    }

    fn test_setup() -> (
        ai_hash::SafHash,
        ai_hash::AgentPubKey,
        ai_hash::AgentPubKey,
        ai_hash::AgentPubKey,
    ) {
        observability::test_run().unwrap();
        (
            newhash!(SafHash, 's'),
            newhash!(AgentPubKey, '1'),
            newhash!(AgentPubKey, '2'),
            newhash!(AgentPubKey, '3'),
        )
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_call_remote_workflow() {
        let (saf, a1, a2, _) = test_setup();

        let (p2p, mut evt) = spawn_aingle_p2p(
            KitsuneP2pConfig::default(),
            TlsConfig::new_ephemeral().await.unwrap(),
        )
        .await
        .unwrap();

        let r_task = tokio::task::spawn(async move {
            use tokio_stream::StreamExt;
            while let Some(evt) = evt.next().await {
                use crate::types::event::AIngleP2pEvent::*;
                match evt {
                    CallRemote { respond, .. } => {
                        respond.r(Ok(
                            async move { Ok(UnsafeBytes::from(b"yada".to_vec()).into()) }
                                .boxed()
                                .into(),
                        ));
                    }
                    SignNetworkData { respond, .. } => {
                        respond.r(Ok(async move { Ok([0; 64].into()) }.boxed().into()));
                    }
                    PutAgentInfoSigned { respond, .. } => {
                        respond.r(Ok(async move { Ok(()) }.boxed().into()));
                    }
                    _ => {}
                }
            }
        });

        p2p.join(saf.clone(), a1.clone()).await.unwrap();
        p2p.join(saf.clone(), a2.clone()).await.unwrap();

        let res = p2p
            .call_remote(
                saf,
                a1,
                a2,
                "".into(),
                "".into(),
                None,
                ExternIO::encode(b"yippo").unwrap(),
            )
            .await
            .unwrap();
        let res: Vec<u8> = UnsafeBytes::from(res).into();

        assert_eq!(b"yada".to_vec(), res);

        p2p.ghost_actor_shutdown().await.unwrap();
        r_task.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_send_validation_receipt_workflow() {
        let (saf, a1, a2, _) = test_setup();

        let (p2p, mut evt) = spawn_aingle_p2p(
            KitsuneP2pConfig::default(),
            TlsConfig::new_ephemeral().await.unwrap(),
        )
        .await
        .unwrap();

        let r_task = tokio::task::spawn(async move {
            use tokio_stream::StreamExt;
            while let Some(evt) = evt.next().await {
                use crate::types::event::AIngleP2pEvent::*;
                match evt {
                    ValidationReceiptReceived {
                        respond, receipt, ..
                    } => {
                        let receipt: Vec<u8> = UnsafeBytes::from(receipt).into();
                        assert_eq!(b"receipt-test".to_vec(), receipt);
                        respond.r(Ok(async move { Ok(()) }.boxed().into()));
                    }
                    SignNetworkData { respond, .. } => {
                        respond.r(Ok(async move { Ok([0; 64].into()) }.boxed().into()));
                    }
                    PutAgentInfoSigned { respond, .. } => {
                        respond.r(Ok(async move { Ok(()) }.boxed().into()));
                    }
                    _ => {}
                }
            }
        });

        p2p.join(saf.clone(), a1.clone()).await.unwrap();
        p2p.join(saf.clone(), a2.clone()).await.unwrap();

        p2p.send_validation_receipt(
            saf,
            a2,
            a1,
            UnsafeBytes::from(b"receipt-test".to_vec()).into(),
        )
        .await
        .unwrap();

        p2p.ghost_actor_shutdown().await.unwrap();
        r_task.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_publish_workflow() {
        let (saf, a1, a2, a3) = test_setup();

        let (p2p, mut evt) = spawn_aingle_p2p(
            KitsuneP2pConfig::default(),
            TlsConfig::new_ephemeral().await.unwrap(),
        )
        .await
        .unwrap();

        let recv_count = Arc::new(std::sync::atomic::AtomicU8::new(0));

        let recv_count_clone = recv_count.clone();
        let r_task = tokio::task::spawn(async move {
            use tokio_stream::StreamExt;
            while let Some(evt) = evt.next().await {
                use crate::types::event::AIngleP2pEvent::*;
                match evt {
                    Publish { respond, .. } => {
                        respond.r(Ok(async move { Ok(()) }.boxed().into()));
                        recv_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                    SignNetworkData { respond, .. } => {
                        respond.r(Ok(async move { Ok([0; 64].into()) }.boxed().into()));
                    }
                    PutAgentInfoSigned { respond, .. } => {
                        respond.r(Ok(async move { Ok(()) }.boxed().into()));
                    }
                    QueryAgentInfoSigned { respond, .. } => {
                        respond.r(Ok(async move { Ok(vec![]) }.boxed().into()));
                    }
                    _ => {}
                }
            }
        });

        p2p.join(saf.clone(), a1.clone()).await.unwrap();
        p2p.join(saf.clone(), a2.clone()).await.unwrap();
        p2p.join(saf.clone(), a3.clone()).await.unwrap();

        let header_hash = ai_hash::AnySgdHash::from_raw_36_and_type(
            b"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".to_vec(),
            ai_hash::hash_type::AnySgd::Header,
        );

        p2p.publish(saf, a1, true, header_hash, vec![], Some(200))
            .await
            .unwrap();

        assert_eq!(3, recv_count.load(std::sync::atomic::Ordering::SeqCst));

        p2p.ghost_actor_shutdown().await.unwrap();
        r_task.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_workflow() {
        observability::test_run().ok();

        let (saf, a1, a2, _a3) = test_setup();

        let (p2p, mut evt) = spawn_aingle_p2p(
            KitsuneP2pConfig::default(),
            TlsConfig::new_ephemeral().await.unwrap(),
        )
        .await
        .unwrap();

        let test_1 = WireOps::Element(WireElementOps {
            header: Some(Judged::valid(SignedHeader(fixt!(Header), fixt!(Signature)))),
            deletes: vec![],
            updates: vec![],
            entry: None,
        });
        let test_2 = WireOps::Element(WireElementOps {
            header: Some(Judged::valid(SignedHeader(fixt!(Header), fixt!(Signature)))),
            deletes: vec![],
            updates: vec![],
            entry: None,
        });

        let mut respond_queue = vec![test_1.clone(), test_2.clone()];
        let r_task = tokio::task::spawn(async move {
            use tokio_stream::StreamExt;
            while let Some(evt) = evt.next().await {
                use crate::types::event::AIngleP2pEvent::*;
                match evt {
                    Get { respond, .. } => {
                        let resp = if let Some(h) = respond_queue.pop() {
                            h
                        } else {
                            panic!("too many requests!")
                        };
                        tracing::info!("test - get respond");
                        respond.r(Ok(async move { Ok(resp) }.boxed().into()));
                    }
                    SignNetworkData { respond, .. } => {
                        respond.r(Ok(async move { Ok([0; 64].into()) }.boxed().into()));
                    }
                    PutAgentInfoSigned { respond, .. } => {
                        respond.r(Ok(async move { Ok(()) }.boxed().into()));
                    }
                    QueryAgentInfoSigned { respond, .. } => {
                        respond.r(Ok(async move { Ok(vec![]) }.boxed().into()));
                    }
                    FetchOpHashesForConstraints { respond, .. } => {
                        respond.r(Ok(async move { Ok(vec![]) }.boxed().into()));
                    }
                    evt => println!("unhandled: {:?}", evt),
                }
            }
        });

        tracing::info!("test - join1");
        p2p.join(saf.clone(), a1.clone()).await.unwrap();
        tracing::info!("test - join2");
        p2p.join(saf.clone(), a2.clone()).await.unwrap();

        let hash = ai_hash::AnySgdHash::from_raw_36_and_type(
            b"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".to_vec(),
            ai_hash::hash_type::AnySgd::Header,
        );

        tracing::info!("test - get");
        let res = p2p
            .get(saf, a1, hash, actor::GetOptions::default())
            .await
            .unwrap();

        tracing::info!("test - check res");
        assert_eq!(2, res.len());

        for r in res {
            assert!(r == test_1 || r == test_2);
        }

        tracing::info!("test - end of test shutdown p2p");
        p2p.ghost_actor_shutdown().await.unwrap();
        tracing::info!("test - end of test await task end");
        r_task.await.unwrap();
        tracing::info!("test - end of test - final done.");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_links_workflow() {
        let (saf, a1, a2, _) = test_setup();

        let (p2p, mut evt) = spawn_aingle_p2p(
            KitsuneP2pConfig::default(),
            TlsConfig::new_ephemeral().await.unwrap(),
        )
        .await
        .unwrap();

        let test_1 = WireLinkOps {
            creates: vec![WireCreateLink::condense(
                fixt!(CreateLink),
                fixt!(Signature),
                ValidationStatus::Valid,
            )],
            deletes: vec![WireDeleteLink::condense(
                fixt!(DeleteLink),
                fixt!(Signature),
                ValidationStatus::Valid,
            )],
        };

        let test_1_clone = test_1.clone();
        let r_task = tokio::task::spawn(async move {
            use tokio_stream::StreamExt;
            while let Some(evt) = evt.next().await {
                let test_1_clone = test_1_clone.clone();
                use crate::types::event::AIngleP2pEvent::*;
                match evt {
                    GetLinks { respond, .. } => {
                        respond.r(Ok(async move { Ok(test_1_clone) }.boxed().into()));
                    }
                    SignNetworkData { respond, .. } => {
                        respond.r(Ok(async move { Ok([0; 64].into()) }.boxed().into()));
                    }
                    PutAgentInfoSigned { respond, .. } => {
                        respond.r(Ok(async move { Ok(()) }.boxed().into()));
                    }
                    _ => {}
                }
            }
        });

        p2p.join(saf.clone(), a1.clone()).await.unwrap();
        p2p.join(saf.clone(), a2.clone()).await.unwrap();

        let hash = ai_hash::EntryHash::from_raw_36_and_type(
            b"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".to_vec(),
            ai_hash::hash_type::Entry,
        );
        let link_key = WireLinkKey {
            base: hash,
            zome_id: 0.into(),
            tag: None,
        };

        let res = p2p
            .get_links(saf, a1, link_key, actor::GetLinksOptions::default())
            .await
            .unwrap();

        assert_eq!(2, res.len());

        for r in res {
            assert_eq!(r, test_1);
        }

        p2p.ghost_actor_shutdown().await.unwrap();
        r_task.await.unwrap();
    }
}
