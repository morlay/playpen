use std::sync::Arc;

use crate::agent::runner::{run_agent_stream, AgentEvent};
use crate::model::Model;
use crate::profile::manager::ProfileManager;
use crate::session::session::{create_session, Session};
use crate::session::store::SessionManager;
use crate::workspace::Workspace;
use crate::tools::{
    read::ReadRigTool, grep::GrepRigTool, edit::EditRigTool,
    write::WriteRigTool, r#move::MoveRigTool, find::FindRigTool,
    bash::BashRigTool, webfetch::WebfetchRigTool,
};
use tokio::sync::mpsc;

pub type EventReceiver = mpsc::UnboundedReceiver<AgentEvent>;

fn build_tools(ws: &Arc<Workspace>) -> Vec<Box<dyn rig_core::tool::ToolDyn>> {
    vec![
        Box::new(ReadRigTool { ws: ws.clone() }),
        Box::new(GrepRigTool { ws: ws.clone() }),
        Box::new(EditRigTool { ws: ws.clone() }),
        Box::new(WriteRigTool { ws: ws.clone() }),
        Box::new(MoveRigTool { ws: ws.clone() }),
        Box::new(FindRigTool { ws: ws.clone() }),
        Box::new(BashRigTool { ws: ws.clone() }),
        Box::new(WebfetchRigTool),
    ]
}

pub struct AgentSession {
    pub session_id: String,
    session: Session,
    client: rig_core::providers::openai::CompletionsClient,
    ws: Arc<Workspace>,
    memory: Option<Arc<dyn rig_core::memory::ConversationMemory>>,
}

impl AgentSession {
    pub fn prompt(&self, user_input: &str) -> EventReceiver {
        run_agent_stream(
            &self.client,
            &self.session,
            user_input,
            build_tools(&self.ws),
            self.memory.clone(),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
    }
}

pub struct AgentSessionService {
    manager: Arc<SessionManager>,
    profile_manager: Arc<ProfileManager>,
    client: rig_core::providers::openai::CompletionsClient,
    ws: Arc<Workspace>,
    memory: Option<Arc<dyn rig_core::memory::ConversationMemory>>,
}

impl AgentSessionService {
    pub fn new(
        manager: Arc<SessionManager>,
        profile_manager: Arc<ProfileManager>,
        client: rig_core::providers::openai::CompletionsClient,
        ws: Arc<Workspace>,
        memory: Option<Arc<dyn rig_core::memory::ConversationMemory>>,
    ) -> Self {
        Self { manager, profile_manager, client, ws, memory }
    }

    pub async fn new_session(&self, model: Model) -> anyhow::Result<AgentSession> {
        let env = self.profile_manager.build_env("default")?;
        let project_root = env.project_root.clone();
        let system_prompt = env.build_system_prompt();

        let session = create_session(
            "new session".into(),
            model,
            project_root,
            "default".into(),
            system_prompt,
            vec![],
            None,
        );
        let id = session.id.clone();
        self.manager.insert(session.clone());
        Ok(AgentSession {
            session_id: id,
            session,
            client: self.client.clone(),
            ws: self.ws.clone(),
            memory: self.memory.clone(),
        })
    }
}
