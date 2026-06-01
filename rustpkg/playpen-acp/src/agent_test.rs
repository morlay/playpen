#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use agent_client_protocol::schema::ProtocolVersion;
    use agent_client_protocol::schema::v1::{
        ContentBlock as AcpContentBlock, EmbeddedResource, EmbeddedResourceResource,
        InitializeRequest, NewSessionRequest, PromptRequest, SessionNotification, SessionUpdate,
        TextContent, TextResourceContents,
    };
    use agent_client_protocol::{ByteStreams, Client};
    use playpen_agent::runner::{AgentRunner, AgentRunnerBuilder, SimpleRunner};
    use playpen_agent::testing::{MockCompletionModel, MockStreamEvent, TestProfile};
    use playpen_config::Settings;
    use playpen_session::SessionService;
    use tokio::sync::mpsc;
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    use crate::agent::serve;

    // ── FakeLlm ──

    #[derive(Clone)]
    struct FakeLlm {
        mock: MockCompletionModel,
    }

    impl FakeLlm {
        fn text(text: &str) -> Self {
            Self {
                mock: MockCompletionModel::from_stream_turns([[MockStreamEvent::text(text)]]),
            }
        }
    }

    // ── FakeRunnerBuilder ──
    struct FakeRunnerBuilder {
        llm: FakeLlm,
        session_service: Arc<dyn playpen_session::SessionService>,
    }

    impl FakeRunnerBuilder {
        async fn make_builder(llm: FakeLlm) -> (Self, Arc<dyn playpen_session::SessionService>) {
            let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
            let ss = playpen_session::DBSessionService::new(db);
            ss.migrate().await.unwrap();
            let svc: Arc<dyn playpen_session::SessionService> = Arc::new(ss);
            (
                Self {
                    llm,
                    session_service: svc.clone(),
                },
                svc,
            )
        }
    }

    #[async_trait]
    impl AgentRunnerBuilder for FakeRunnerBuilder {
        async fn create(
            &self,
            p: Box<dyn playpen_profile::AgentProfile>,
        ) -> anyhow::Result<Box<dyn AgentRunner>> {
            let session = self.session_service.create().await?;
            let sid = session.id().to_string();
            let inner = SimpleRunner::new(
                sid,
                session,
                p,
                Settings::default(),
                self.session_service.clone(),
            );
            Ok(Box::new(FakeRunner {
                inner,
                llm: self.llm.clone(),
            }))
        }
        async fn resume(&self, id: &str) -> anyhow::Result<Box<dyn AgentRunner>> {
            let session = self
                .session_service
                .get(id)
                .await
                .map_err(|_| anyhow::anyhow!("session {id} 不存在"))?;

            let profile: Box<dyn playpen_profile::AgentProfile> = Box::new(TestProfile);
            let inner = SimpleRunner::new(
                id.to_string(),
                session,
                profile,
                Settings::default(),
                self.session_service.clone(),
            );
            Ok(Box::new(FakeRunner {
                inner,
                llm: self.llm.clone(),
            }))
        }
        fn agent_profiles(&self) -> anyhow::Result<Vec<Box<dyn playpen_profile::AgentProfile>>> {
            Ok(vec![Box::new(TestProfile)])
        }
        fn sessions(&self) -> &dyn SessionService {
            panic!("sessions() not available")
        }
    }

    struct FakeRunner {
        inner: SimpleRunner,
        llm: FakeLlm,
    }

    #[async_trait]
    impl AgentRunner for FakeRunner {
        fn id(&self) -> &str {
            self.inner.id()
        }
        fn session(&self) -> &dyn playpen_session::Session {
            self.inner.session()
        }
        fn profile(&self) -> &dyn playpen_profile::AgentProfile {
            self.inner.profile()
        }
        fn settings(&self) -> &Settings {
            self.inner.settings()
        }
        fn with_profile(&self, p: Box<dyn playpen_profile::AgentProfile>) -> Box<dyn AgentRunner> {
            self.inner.with_profile(p)
        }
        async fn run(
            &self,
            prompt: Vec<playpen_content::ContentBlock>,
        ) -> std::pin::Pin<
            Box<dyn futures::Stream<Item = playpen_content::Event> + std::marker::Send>,
        > {
            self.inner
                .run_with_model(self.llm.mock.clone(), prompt, vec![], None, |_| None)
                .await
        }
        async fn rewind(&self) -> anyhow::Result<()> {
            self.inner.rewind().await
        }
        fn replay(
            &self,
        ) -> std::pin::Pin<
            Box<dyn futures::Stream<Item = playpen_content::Event> + std::marker::Send>,
        > {
            self.inner.replay()
        }
        async fn cancel(&self) {
            self.inner.cancel().await
        }
    }

    fn make_transport_pair() -> (
        impl agent_client_protocol::ConnectTo<agent_client_protocol::Agent> + 'static,
        impl agent_client_protocol::ConnectTo<agent_client_protocol::Client> + 'static,
    ) {
        let (rx_from_server, tx_to_client) = tokio::io::simplex(8192);
        let (rx_from_client, tx_to_server) = tokio::io::simplex(8192);

        let server_tr = ByteStreams::new(tx_to_client.compat_write(), rx_from_client.compat());
        let client_tr = ByteStreams::new(tx_to_server.compat_write(), rx_from_server.compat());
        (server_tr, client_tr)
    }

    #[tokio::test]
    async fn test_full_acp_event_flow() {
        let (builder, _svc) =
            FakeRunnerBuilder::make_builder(FakeLlm::text("hello from acp")).await;
        let builder: Box<dyn AgentRunnerBuilder> = Box::new(builder);
        let (server_tr, client_tr) = make_transport_pair();

        let (notify_tx, mut notify_rx) = mpsc::unbounded_channel::<SessionNotification>();

        let server_handle = tokio::spawn(async move {
            let result = serve(builder, server_tr).await;
            if let Err(ref e) = result {
                eprintln!("serve error: {e}");
            }
            result
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let client_result = tokio::spawn(async move {
            let notify_tx_clone = notify_tx.clone();

            Client
                .builder()
                .on_receive_notification(
                    async move |notif: SessionNotification, _cx| {
                        let _ = notify_tx_clone.send(notif);
                        Ok(())
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                .connect_with(
                    client_tr,
                    |connection: agent_client_protocol::ConnectionTo<
                        agent_client_protocol::Agent,
                    >| async move {
                        let _init = connection
                            .send_request(InitializeRequest::new(ProtocolVersion::V1))
                            .block_task()
                            .await?;
                        let new = connection
                            .send_request(NewSessionRequest::new(PathBuf::from("/tmp")))
                            .block_task()
                            .await?;
                        let _prompt = connection
                            .send_request(PromptRequest::new(
                                new.session_id,
                                vec![AcpContentBlock::Text(TextContent::new("hello"))],
                            ))
                            .block_task()
                            .await?;
                        Ok::<_, agent_client_protocol::Error>(())
                    },
                )
                .await
        });

        let client_res = client_result.await.unwrap();
        assert!(client_res.is_ok(), "client failed: {:?}", client_res.err());

        let mut notifications = Vec::new();
        while let Ok(notif) = notify_rx.try_recv() {
            notifications.push(notif);
        }
        assert!(
            !notifications.is_empty(),
            "应收到至少一条 ACP SessionNotification"
        );

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!server_handle.is_finished());
        server_handle.abort();
    }

    #[tokio::test]
    async fn test_multi_segment_prompt_with_resource_block() {
        let (builder, _svc) = FakeRunnerBuilder::make_builder(FakeLlm::text("ok")).await;
        let builder: Box<dyn AgentRunnerBuilder> = Box::new(builder);
        let (server_tr, client_tr) = make_transport_pair();

        let (notify_tx, mut notify_rx) = mpsc::unbounded_channel::<SessionNotification>();
        let notify_tx_for_client = notify_tx.clone();

        let server_handle = tokio::spawn(async move {
            let result = serve(builder, server_tr).await;
            if let Err(ref e) = result {
                eprintln!("serve error: {e}");
            }
            result
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let client_result = tokio::spawn(async move {
            Client
                .builder()
                .on_receive_notification(
                    {
                        let tx = notify_tx_for_client.clone();
                        async move |notif: SessionNotification, _cx| {
                            let _ = tx.send(notif);
                            Ok(())
                        }
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                .connect_with(
                    client_tr,
                    |connection: agent_client_protocol::ConnectionTo<
                        agent_client_protocol::Agent,
                    >| async move {
                        let _init = connection
                            .send_request(InitializeRequest::new(ProtocolVersion::V1))
                            .block_task()
                            .await?;
                        let new = connection
                            .send_request(NewSessionRequest::new(PathBuf::from("/tmp")))
                            .block_task()
                            .await?;

                        // 发送多段 prompt：text + resource
                        let prompt = connection
                            .send_request(PromptRequest::new(
                                new.session_id,
                                vec![
                                    AcpContentBlock::Text(TextContent::new("读一下 ")),
                                    AcpContentBlock::Resource(EmbeddedResource::new(
                                        EmbeddedResourceResource::TextResourceContents(
                                            TextResourceContents::new(
                                                "[tools]\nrust = \"1.95.0\"\n",
                                                "file:///dev/null/mise.toml",
                                            ),
                                        ),
                                    )),
                                ],
                            ))
                            .block_task()
                            .await?;

                        // 验证 prompt 成功
                        let reason = format!("{:?}", prompt.stop_reason);
                        assert!(
                            reason.contains("EndTurn"),
                            "multi-segment prompt 应成功, got: {reason}"
                        );

                        Ok::<_, agent_client_protocol::Error>(())
                    },
                )
                .await
        });

        let client_res = client_result.await.unwrap();
        assert!(client_res.is_ok(), "client failed: {:?}", client_res.err());

        tokio::time::sleep(Duration::from_millis(200)).await;
        drop(notify_tx);

        let notifications: Vec<_> = notify_rx.try_recv().into_iter().collect();
        // 不应有 UserMessageChunk（非 rewind prompt 不会产生）
        assert!(
            !notifications.iter().any(|n: &SessionNotification| matches!(
                n.update,
                SessionUpdate::UserMessageChunk(_)
            )),
            "非 rewind prompt 不应发射 UserMessageChunk"
        );

        assert!(!server_handle.is_finished());
        server_handle.abort();
    }
}
