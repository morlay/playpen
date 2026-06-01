use std::path::PathBuf;

use agent_client_protocol::schema::v1::{ContentBlock, ResourceLink, TextContent};
use playpen_profile::skill::{self, LocalSkill, Skill};

use crate::slash_command::process_slash_commands;

fn make_skill(name: &str) -> Box<dyn Skill> {
    let metadata = skill::Metadata {
        name: name.to_string(),
        description: format!("skill {} 的描述", name),
        license: None,
        metadata: None,
        disable_model_invocation: None,
    };
    Box::new(LocalSkill::new(
        metadata,
        PathBuf::from(format!("/tmp/skills/{name}/SKILL.md")),
        skill::Source::Global,
        String::new(),
    ))
}

fn make_skill_link(name: &str) -> ContentBlock {
    let uri = format!("file:///tmp/skills/{name}/SKILL.md");
    ContentBlock::ResourceLink(
        ResourceLink::new(format!("skill:{name}"), uri)
            .description(Some(format!("skill {} 的描述", name))),
    )
}

// ── rewind ─────────────────────────────────────────────────────────────

#[test]
fn test_rewind_removes_prefix_keeps_rest() {
    let blocks = vec![make_text("/rewind 重新分析")];
    let (result, rewind) = process_slash_commands(blocks, &[]);
    assert!(rewind);
    assert_eq!(result, vec![make_text("重新分析")]);
}

#[test]
fn test_rewind_pure_no_text() {
    let blocks = vec![make_text("/rewind")];
    let (result, rewind) = process_slash_commands(blocks, &[]);
    assert!(rewind);
    assert!(result.is_empty());
}

#[test]
fn test_rewind_with_resource_blocks_preserved() {
    let link = make_skill_link("code");
    let blocks = vec![make_text("/rewind"), link.clone()];
    let (result, rewind) = process_slash_commands(blocks, &[]);
    assert!(rewind);
    // /rewind text block 被完全移除，resource link 保留
    assert_eq!(result, vec![link]);
}

#[test]
fn test_rewind_with_trailing_newlines() {
    let blocks = vec![make_text("/rewind\n\n新内容")];
    let (result, rewind) = process_slash_commands(blocks, &[]);
    assert!(rewind);
    // trim 后的结果，空白被清理
    assert_eq!(result, vec![make_text("新内容")]);
}

// ── skill ──────────────────────────────────────────────────────────────

#[test]
fn test_skill_replaced_with_link() {
    let skills = vec![make_skill("code")];
    let blocks = vec![make_text("/code 分析 main.rs")];
    let (result, rewind) = process_slash_commands(blocks, &skills);
    assert!(!rewind);
    assert_eq!(
        result,
        vec![make_skill_link("code"), make_text("分析 main.rs")]
    );
}

#[test]
fn test_skill_no_args() {
    let skills = vec![make_skill("debug")];
    let blocks = vec![make_text("/debug")];
    let (result, rewind) = process_slash_commands(blocks, &skills);
    assert!(!rewind);
    assert_eq!(result, vec![make_skill_link("debug")]);
}

#[test]
fn test_skill_not_found_keeps_original() {
    let skills = vec![make_skill("code")];
    let blocks = vec![make_text("/unknown 参数")];
    let (result, rewind) = process_slash_commands(blocks, &skills);
    assert!(!rewind);
    assert_eq!(result, vec![make_text("/unknown 参数")]);
}

// ── mixed ──────────────────────────────────────────────────────────────

#[test]
fn test_rewind_and_skill_mixed() {
    let skills = vec![make_skill("code")];
    let blocks = vec![make_text("/rewind"), make_text("/code 分析 main.rs")];
    let (result, rewind) = process_slash_commands(blocks, &skills);
    assert!(rewind);
    // /rewind 被完全移除，/code 被替换
    assert_eq!(
        result,
        vec![make_skill_link("code"), make_text("分析 main.rs")]
    );
}

#[test]
fn test_plain_text_unchanged() {
    let blocks = vec![make_text("普通文本"), make_text("另一行")];
    let (result, rewind) = process_slash_commands(blocks, &[]);
    assert!(!rewind);
    assert_eq!(result, vec![make_text("普通文本"), make_text("另一行")]);
}

// ── path-like ──────────────────────────────────────────────────────────

#[test]
fn test_path_like_not_treated_as_command() {
    let skills = vec![make_skill("code")];
    // 路径包含斜杠，不应误判为 slash command
    let blocks = vec![make_text("/foo/bar")];
    let (result, rewind) = process_slash_commands(blocks, &skills);
    assert!(!rewind);
    assert_eq!(result, vec![make_text("/foo/bar")]);
}

#[test]
fn test_path_like_with_known_prefix_not_matched() {
    let skills = vec![make_skill("usr")];
    // /usr/local/bin 整体不是合法 DNS 标签，不会被解析
    let blocks = vec![make_text("/usr/local/bin")];
    let (result, rewind) = process_slash_commands(blocks, &skills);
    assert!(!rewind);
    // 不会匹配到 skill "usr"
    assert_eq!(result, vec![make_text("/usr/local/bin")]);
}

// ── quoted / code-block ────────────────────────────────────────────────

#[test]
fn test_quoted_rewind_not_matched() {
    let blocks = vec![make_text("\"/rewind\"")];
    let (result, rewind) = process_slash_commands(blocks, &[]);
    assert!(!rewind);
    assert_eq!(result, vec![make_text("\"/rewind\"")]);
}

#[test]
fn test_code_block_rewind_not_matched() {
    let blocks = vec![make_text("```/rewind```")];
    let (result, rewind) = process_slash_commands(blocks, &[]);
    assert!(!rewind);
    assert_eq!(result, vec![make_text("```/rewind```")]);
}

#[test]
fn test_quoted_skill_not_matched() {
    let skills = vec![make_skill("code")];
    let blocks = vec![make_text("\"/code 分析\"")];
    let (result, rewind) = process_slash_commands(blocks, &skills);
    assert!(!rewind);
    assert_eq!(result, vec![make_text("\"/code 分析\"")]);
}

// ── Weak 生命周期 ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_weak_lifecycle() {
    use std::sync::Arc;

    use async_trait::async_trait;

    use playpen_agent::runner::{AgentRunner, AgentRunnerBuilder};

    /// 最小化 stub，只验证 Weak 生命周期，不涉及实际 runner 方法调用
    struct StubRunner(&'static str);

    #[async_trait]
    impl AgentRunner for StubRunner {
        fn id(&self) -> &str {
            self.0
        }
        fn session(&self) -> &dyn playpen_session::Session {
            unimplemented!()
        }
        fn profile(&self) -> &dyn playpen_profile::AgentProfile {
            unimplemented!()
        }
        fn settings(&self) -> &playpen_config::Settings {
            unimplemented!()
        }
        fn with_profile(&self, _: Box<dyn playpen_profile::AgentProfile>) -> Box<dyn AgentRunner> {
            unimplemented!()
        }
        async fn run(
            &self,
            _: Vec<playpen_content::ContentBlock>,
        ) -> std::pin::Pin<Box<dyn futures::Stream<Item = playpen_content::Event> + Send>> {
            unimplemented!()
        }
        async fn rewind(&self) -> anyhow::Result<()> {
            unimplemented!()
        }
        fn replay(
            &self,
        ) -> std::pin::Pin<Box<dyn futures::Stream<Item = playpen_content::Event> + Send>> {
            unimplemented!()
        }
        async fn cancel(&self) {}
    }

    struct StubBuilder;

    #[async_trait]
    impl AgentRunnerBuilder for StubBuilder {
        async fn create(
            &self,
            _: Box<dyn playpen_profile::AgentProfile>,
        ) -> anyhow::Result<Box<dyn AgentRunner>> {
            unimplemented!()
        }
        async fn resume(&self, _: &str) -> anyhow::Result<Box<dyn AgentRunner>> {
            unimplemented!()
        }
        fn agent_profiles(&self) -> anyhow::Result<Vec<Box<dyn playpen_profile::AgentProfile>>> {
            unimplemented!()
        }
        fn sessions(&self) -> &dyn playpen_session::SessionService {
            unimplemented!()
        }
    }

    let state = super::AcpState::new(Box::new(StubBuilder));

    let runner: Box<dyn AgentRunner> = Box::new(StubRunner("test-sid"));
    let runner_arc = Arc::new(runner);

    state.register_running("test-sid", runner_arc.clone()).await;

    // 运行中应能找到
    {
        let found = state.get_runner("test-sid").await;
        assert!(found.is_some(), "运行中应能获取到 runner");
    }

    // 模拟 prompt 结束：释放强引用
    drop(runner_arc);

    // Weak 已过期，应返回 None
    let found = state.get_runner("test-sid").await;
    assert!(found.is_none(), "prompt 结束后应获取不到 runner");
}

// ── helpers ────────────────────────────────────────────────────────────

fn make_text(text: &str) -> ContentBlock {
    ContentBlock::Text(TextContent::new(text.to_string()))
}
