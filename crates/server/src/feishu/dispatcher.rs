use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, OnceLock},
    time::Duration,
};

use anyhow::{Context, Result};
use db::models::{
    coding_agent_turn::CodingAgentTurn,
    execution_process::{ExecutionProcess, ExecutionProcessRunReason, ExecutionProcessStatus},
    session::Session,
    workspace::{Workspace, WorkspaceWithStatus},
    workspace_feishu_binding::{UpdateWorkspaceFeishuBinding, WorkspaceFeishuBinding},
};
use deployment::Deployment;
use json_patch::PatchOperation;
use tokio::sync::{Mutex, broadcast::error::RecvError};
use utils::log_msg::LogMsg;
use uuid::Uuid;

use crate::feishu::{
    chat_runner::{DeploymentFeishuChatRunner, FeishuChatRunner, NoopFeishuChatRunner},
    client::{FeishuClient, HttpFeishuClient},
    secret_store::FileFeishuSecretStore,
    service::FeishuService,
};

const EXECUTION_REPLY_POLL_ATTEMPTS: usize = if cfg!(test) { 3 } else { 150 };
const EXECUTION_REPLY_POLL_INTERVAL: Duration = if cfg!(test) {
    Duration::from_millis(1)
} else {
    Duration::from_millis(200)
};
const DEFAULT_ACK_REACTION_EMOJI_TYPE: &str = "Typing";
const SUPPORTED_ACK_REACTION_EMOJI_TYPES: &[&str] =
    &["Typing", "OnIt", "OK", "THUMBSUP", "CheckMark"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FeishuAnnouncementState {
    Running,
    Idle,
    NeedsAttention,
}

#[derive(Clone)]
pub struct FeishuDispatcher<S, C>
where
    S: crate::feishu::secret_store::FeishuSecretStore,
    C: FeishuClient,
{
    service: Arc<FeishuService<S>>,
    client: Arc<C>,
    chat_runner: Arc<dyn FeishuChatRunner>,
    processed_event_ids: Arc<Mutex<HashSet<String>>>,
    processed_message_ids: Arc<Mutex<HashSet<String>>>,
    delivered_execution_replies: Arc<Mutex<HashSet<Uuid>>>,
    announcement_signatures: Arc<Mutex<HashMap<Uuid, String>>>,
}

impl<S, C> FeishuDispatcher<S, C>
where
    S: crate::feishu::secret_store::FeishuSecretStore + 'static,
    C: FeishuClient + 'static,
{
    pub fn new(service: Arc<FeishuService<S>>, client: Arc<C>) -> Self {
        Self::with_chat_runner(service, client, Arc::new(NoopFeishuChatRunner))
    }

    pub fn with_chat_runner(
        service: Arc<FeishuService<S>>,
        client: Arc<C>,
        chat_runner: Arc<dyn FeishuChatRunner>,
    ) -> Self {
        Self {
            service,
            client,
            chat_runner,
            processed_event_ids: Arc::new(Mutex::new(HashSet::new())),
            processed_message_ids: Arc::new(Mutex::new(HashSet::new())),
            delivered_execution_replies: Arc::new(Mutex::new(HashSet::new())),
            announcement_signatures: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn dispatch_workspace_status(&self, workspace: &Workspace) -> Result<usize> {
        let bindings = self.service.list_bindings(workspace.id).await?;
        let workspace_with_status = if bindings
            .iter()
            .any(|binding| binding.sync_group_announcement)
        {
            Workspace::find_by_id_with_status(self.service.pool(), workspace.id).await?
        } else {
            None
        };
        let mut sent = 0;

        for binding in bindings.into_iter().filter(|binding| binding.enabled) {
            if binding.sync_group_announcement
                && let Some(workspace_with_status) = workspace_with_status.as_ref()
            {
                self.sync_group_announcement_for_binding(
                    binding.clone(),
                    workspace_with_status,
                    None,
                )
                .await;
            }

            if binding.notify_on_status && !binding.sync_group_announcement {
                let message = format!(
                    "Workspace {} status updated. Archived: {}. Pinned: {}. Branch: {}.",
                    workspace.name.as_deref().unwrap_or(&workspace.branch),
                    workspace.archived,
                    workspace.pinned,
                    workspace.branch
                );
                self.service
                    .send_message_to_target(
                        self.client.as_ref(),
                        binding.bot_id,
                        binding.target_id,
                        Some(workspace.id),
                        "workspace_status",
                        &message,
                    )
                    .await?;
                sent += 1;
            }
        }

        Ok(sent)
    }

    pub async fn handle_inbound_command(
        &self,
        event_id: &str,
        workspace_id: Uuid,
        binding_id: Uuid,
        command: &str,
    ) -> Result<String> {
        if self.is_duplicate_event(event_id).await {
            return Ok("duplicate".to_string());
        }

        let binding = self.get_binding(workspace_id, binding_id).await?;
        self.execute_command(binding, command).await
    }

    pub async fn dispatch_inbound_chat_message(
        &self,
        event_id: &str,
        bot_id: Uuid,
        chat_id: Option<&str>,
        open_chat_id: Option<&str>,
        message_id: Option<&str>,
        text: &str,
    ) -> Result<Option<String>> {
        if self.is_duplicate_event(event_id).await {
            return Ok(None);
        }
        if self.is_duplicate_message(message_id).await {
            return Ok(None);
        }

        let Some(target) = self
            .service
            .find_target_by_identity(bot_id, open_chat_id, chat_id)
            .await?
        else {
            return Ok(None);
        };

        let bindings = self.active_bindings_for_target(bot_id, target.id).await?;

        if let Some(binding) = bindings.iter().find(|binding| binding.ack_reaction_enabled) {
            self.try_acknowledge_message_receipt(binding, message_id)
                .await;
        }

        if let Some(command) = extract_command(text) {
            let reply = match bindings.as_slice() {
                [] => return Ok(None),
                [binding] => self.execute_command(binding.clone(), command).await?,
                _ => "Multiple workspaces are bound to this Feishu chat. Keep one active binding per target before using chat commands.".to_string(),
            };
            let workspace_id = if bindings.len() == 1 {
                bindings.first().map(|binding| binding.workspace_id)
            } else {
                None
            };

            self.service
                .send_message_to_target(
                    self.client.as_ref(),
                    bot_id,
                    target.id,
                    workspace_id,
                    "command_reply",
                    &reply,
                )
                .await?;

            return Ok(Some(reply));
        }

        match bindings.as_slice() {
            [] => Ok(None),
            [binding] if !binding.allow_chat_messages => Ok(None),
            [binding] => {
                let conversation = self
                    .service
                    .ensure_chat_session_for_binding(binding)
                    .await?;
                let session_id = conversation
                    .session_id
                    .context("Feishu conversation session missing")?;

                match self
                    .chat_runner
                    .submit_workspace_chat_message(session_id, binding.workspace_id, text.trim())
                    .await
                {
                    Ok(()) => {
                        if let Some(workspace_with_status) = Workspace::find_by_id_with_status(
                            self.service.pool(),
                            binding.workspace_id,
                        )
                        .await?
                        {
                            self.sync_group_announcement_for_binding(
                                binding.clone(),
                                &workspace_with_status,
                                Some(FeishuAnnouncementState::Running),
                            )
                            .await;
                        }

                        Ok(None)
                    }
                    Err(error) => {
                        let reply =
                            format!("Failed to forward message into workspace chat: {error}");
                        self.service
                            .send_message_to_target(
                                self.client.as_ref(),
                                bot_id,
                                target.id,
                                Some(binding.workspace_id),
                                "chat_forward_error",
                                &reply,
                            )
                            .await?;
                        if let Some(workspace_with_status) = Workspace::find_by_id_with_status(
                            self.service.pool(),
                            binding.workspace_id,
                        )
                        .await?
                        {
                            self.sync_group_announcement_for_binding(
                                binding.clone(),
                                &workspace_with_status,
                                Some(FeishuAnnouncementState::NeedsAttention),
                            )
                            .await;
                        }
                        Ok(Some(reply))
                    }
                }
            }
            _ => {
                let reply = "Multiple workspaces are bound to this Feishu chat. Keep one active binding per target before using chat mode.".to_string();
                self.service
                    .send_message_to_target(
                        self.client.as_ref(),
                        bot_id,
                        target.id,
                        None,
                        "chat_mode_rejected",
                        &reply,
                    )
                    .await?;
                Ok(Some(reply))
            }
        }
    }

    pub async fn dispatch_execution_process_reply(
        &self,
        execution_process_id: Uuid,
    ) -> Result<usize> {
        let Some(process) =
            ExecutionProcess::find_by_id(self.service.pool(), execution_process_id).await?
        else {
            return Ok(0);
        };

        if process.dropped || process.run_reason != ExecutionProcessRunReason::CodingAgent {
            return Ok(0);
        }

        let Some(session) = Session::find_by_id(self.service.pool(), process.session_id).await?
        else {
            return Ok(0);
        };

        let bindings = self.bindings_for_session(session.id).await?;
        self.sync_process_announcements(&process, &bindings).await;

        if process.status == ExecutionProcessStatus::Running {
            return Ok(0);
        }

        let Some(reply) = self.execution_reply_message(&process).await? else {
            return Ok(0);
        };

        {
            let mut delivered = self.delivered_execution_replies.lock().await;
            if !delivered.insert(execution_process_id) {
                return Ok(0);
            }
        }

        let mut sent = 0;
        let mut sent_targets = HashSet::new();

        let send_result = async {
            for binding in bindings {
                if !binding.enabled || !binding.notify_on_agent_reply {
                    continue;
                }

                if !sent_targets.insert((binding.bot_id, binding.target_id)) {
                    continue;
                }

                self.service
                    .send_message_to_target(
                        self.client.as_ref(),
                        binding.bot_id,
                        binding.target_id,
                        Some(binding.workspace_id),
                        "agent_reply",
                        &reply,
                    )
                    .await?;
                sent += 1;
            }

            Ok(sent)
        }
        .await;

        if send_result.is_err() {
            self.delivered_execution_replies
                .lock()
                .await
                .remove(&execution_process_id);
        }

        send_result
    }

    async fn execute_command(
        &self,
        binding: WorkspaceFeishuBinding,
        command: &str,
    ) -> Result<String> {
        let workspace = Workspace::find_by_id(self.service.pool(), binding.workspace_id)
            .await?
            .context("Workspace not found")?;

        match command.trim() {
            "/vk help" => Ok("Available commands: /vk help, /vk status, /vk open, /vk stop, /vk archive, /vk new".to_string()),
            "/vk status" => Ok(format!(
                "Workspace {} — archived={}, pinned={}, branch={}",
                workspace.name.as_deref().unwrap_or(&workspace.branch),
                workspace.archived,
                workspace.pinned,
                workspace.branch
            )),
            other if !binding.allow_commands => Ok(format!(
                "Commands are disabled for this binding. Rejected command: {}",
                other
            )),
            "/vk open" => Ok(format!("Open workspace {}", workspace.id)),
            "/vk stop" => Ok(format!("Stop requested for workspace {}", workspace.id)),
            "/vk archive" => {
                Workspace::update(self.service.pool(), workspace.id, Some(true), None, None)
                    .await?;
                Ok(format!("Archived workspace {}", workspace.id))
            }
            "/vk new" => Ok(
                "Workspace creation from Feishu requires a saved template in Vibe Kanban."
                    .to_string(),
            ),
            other => Ok(format!("Unsupported command: {}", other)),
        }
    }

    async fn try_acknowledge_message_receipt(
        &self,
        binding: &WorkspaceFeishuBinding,
        message_id: Option<&str>,
    ) {
        let Some(message_id) = message_id.map(str::trim).filter(|value| !value.is_empty()) else {
            return;
        };
        let emoji_type = normalized_ack_reaction_emoji_type(&binding.ack_reaction_emoji_type);

        if let Err(error) = self
            .service
            .add_message_reaction(self.client.as_ref(), binding.bot_id, message_id, emoji_type)
            .await
        {
            tracing::warn!(
                ?error,
                binding_id = %binding.id,
                message_id,
                "Failed to acknowledge Feishu message receipt"
            );
        }
    }

    async fn bindings_for_session(&self, session_id: Uuid) -> Result<Vec<WorkspaceFeishuBinding>> {
        let conversations = self
            .service
            .list_conversations_for_session(session_id)
            .await?;
        let mut seen = HashSet::new();
        let mut bindings = Vec::new();

        for conversation in conversations {
            if !seen.insert((conversation.workspace_id, conversation.target_id)) {
                continue;
            }

            if let Some(binding) = WorkspaceFeishuBinding::find_by_workspace_and_target(
                self.service.pool(),
                conversation.workspace_id,
                conversation.target_id,
            )
            .await?
            {
                bindings.push(binding);
            }
        }

        Ok(bindings)
    }

    async fn sync_process_announcements(
        &self,
        process: &ExecutionProcess,
        bindings: &[WorkspaceFeishuBinding],
    ) {
        let Some(session) = Session::find_by_id(self.service.pool(), process.session_id)
            .await
            .ok()
            .flatten()
        else {
            return;
        };

        let Some(workspace_with_status) =
            Workspace::find_by_id_with_status(self.service.pool(), session.workspace_id)
                .await
                .ok()
                .flatten()
        else {
            return;
        };

        let override_state = match process.status {
            ExecutionProcessStatus::Running => Some(FeishuAnnouncementState::Running),
            ExecutionProcessStatus::Failed | ExecutionProcessStatus::Killed => {
                Some(FeishuAnnouncementState::NeedsAttention)
            }
            ExecutionProcessStatus::Completed => None,
        };

        for binding in bindings.iter().filter(|binding| binding.enabled) {
            self.sync_group_announcement_for_binding(
                binding.clone(),
                &workspace_with_status,
                override_state,
            )
            .await;
        }
    }

    async fn sync_group_announcement_for_binding(
        &self,
        binding: WorkspaceFeishuBinding,
        workspace_with_status: &WorkspaceWithStatus,
        override_state: Option<FeishuAnnouncementState>,
    ) {
        if !binding.sync_group_announcement {
            return;
        }

        let content = format_group_announcement_content(workspace_with_status, override_state);
        {
            let signatures = self.announcement_signatures.lock().await;
            if signatures
                .get(&binding.id)
                .is_some_and(|previous| previous == &content)
            {
                return;
            }
        }

        match self
            .service
            .upsert_status_announcement(
                self.client.as_ref(),
                binding.bot_id,
                binding.target_id,
                &content,
            )
            .await
        {
            Ok(()) => {
                self.announcement_signatures
                    .lock()
                    .await
                    .insert(binding.id, content);
            }
            Err(error) => {
                tracing::warn!(
                    ?error,
                    binding_id = %binding.id,
                    workspace_id = %binding.workspace_id,
                    "Failed to sync Feishu group announcement"
                );
            }
        }
    }

    pub async fn handle_card_action(
        &self,
        event_id: &str,
        workspace_id: Uuid,
        binding_id: Uuid,
        action: &str,
    ) -> Result<String> {
        if self.is_duplicate_event(event_id).await {
            return Ok("duplicate".to_string());
        }

        let binding = self.get_binding(workspace_id, binding_id).await?;
        if !binding.allow_cards {
            return Ok("Card interactions are disabled for this binding".to_string());
        }

        match action {
            "open_workspace" => Ok(format!("Open workspace {}", workspace_id)),
            "archive_workspace" => {
                Workspace::update(self.service.pool(), workspace_id, Some(true), None, None)
                    .await?;
                Ok(format!("Archived workspace {}", workspace_id))
            }
            "disable_commands" => {
                self.service
                    .update_binding(
                        binding.id,
                        UpdateWorkspaceFeishuBinding {
                            enabled: binding.enabled,
                            notify_on_status: binding.notify_on_status,
                            notify_on_agent_reply: binding.notify_on_agent_reply,
                            notify_on_pr: binding.notify_on_pr,
                            allow_commands: false,
                            allow_chat_messages: binding.allow_chat_messages,
                            allow_cards: binding.allow_cards,
                            ack_reaction_enabled: binding.ack_reaction_enabled,
                            ack_reaction_emoji_type: binding.ack_reaction_emoji_type.clone(),
                            sync_group_announcement: binding.sync_group_announcement,
                        },
                    )
                    .await?;
                Ok("Disabled commands for this binding".to_string())
            }
            other => Ok(format!("Unsupported card action: {}", other)),
        }
    }

    async fn active_bindings_for_target(
        &self,
        bot_id: Uuid,
        target_id: Uuid,
    ) -> Result<Vec<WorkspaceFeishuBinding>> {
        let mut bindings = self
            .service
            .list_bindings_for_target(target_id)
            .await?
            .into_iter()
            .filter(|binding| binding.enabled && binding.bot_id == bot_id)
            .collect::<Vec<_>>();

        if let Some(primary_binding) = bindings.first()
            && bindings.len() > 1
        {
            self.service
                .ensure_single_active_binding_for_target(bot_id, target_id, primary_binding.id)
                .await?;
            bindings = self
                .service
                .list_bindings_for_target(target_id)
                .await?
                .into_iter()
                .filter(|binding| binding.enabled && binding.bot_id == bot_id)
                .collect::<Vec<_>>();
        }

        Ok(bindings)
    }

    async fn is_duplicate_event(&self, event_id: &str) -> bool {
        let mut processed = self.processed_event_ids.lock().await;
        !processed.insert(event_id.to_string())
    }

    async fn is_duplicate_message(&self, message_id: Option<&str>) -> bool {
        let Some(message_id) = message_id.map(str::trim).filter(|value| !value.is_empty()) else {
            return false;
        };
        let mut processed = self.processed_message_ids.lock().await;
        !processed.insert(message_id.to_string())
    }

    async fn get_binding(
        &self,
        workspace_id: Uuid,
        binding_id: Uuid,
    ) -> Result<WorkspaceFeishuBinding> {
        self.service
            .list_bindings(workspace_id)
            .await?
            .into_iter()
            .find(|binding| binding.id == binding_id)
            .context("Feishu binding not found")
    }

    async fn execution_reply_message(&self, process: &ExecutionProcess) -> Result<Option<String>> {
        for _ in 0..EXECUTION_REPLY_POLL_ATTEMPTS {
            if let Some(turn) =
                CodingAgentTurn::find_by_execution_process_id(self.service.pool(), process.id)
                    .await?
                && let Some(summary) = turn.summary
            {
                let trimmed = summary.trim();
                if !trimmed.is_empty() {
                    return Ok(Some(trimmed.to_string()));
                }
            }

            if process.status != ExecutionProcessStatus::Completed {
                break;
            }

            tokio::time::sleep(EXECUTION_REPLY_POLL_INTERVAL).await;
        }

        let fallback = match process.status {
            ExecutionProcessStatus::Completed => return Ok(None),
            ExecutionProcessStatus::Failed => {
                "The workspace run failed before a final assistant reply was produced.".to_string()
            }
            ExecutionProcessStatus::Killed => {
                "The workspace run was stopped before a final assistant reply was produced."
                    .to_string()
            }
            ExecutionProcessStatus::Running => return Ok(None),
        };

        Ok(Some(fallback))
    }
}

fn extract_command(text: &str) -> Option<&str> {
    let command_start = text.find("/vk")?;
    let command = text[command_start..].trim();
    if command.is_empty() {
        None
    } else {
        Some(command)
    }
}

type GlobalFeishuDispatcher = FeishuDispatcher<FileFeishuSecretStore, HttpFeishuClient>;

static GLOBAL_DISPATCHER: OnceLock<GlobalFeishuDispatcher> = OnceLock::new();

pub fn init_global_dispatcher(
    deployment: &crate::DeploymentImpl,
) -> &'static GlobalFeishuDispatcher {
    GLOBAL_DISPATCHER.get_or_init(|| {
        let service = Arc::new(FeishuService::from_deployment(deployment));
        FeishuDispatcher::with_chat_runner(
            service,
            Arc::new(HttpFeishuClient::new()),
            Arc::new(DeploymentFeishuChatRunner::new(deployment.clone())),
        )
    })
}

pub fn global_dispatcher() -> Option<&'static GlobalFeishuDispatcher> {
    GLOBAL_DISPATCHER.get()
}

pub fn spawn_global_workspace_event_bridge(
    deployment: crate::DeploymentImpl,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Some(dispatcher) = global_dispatcher() else {
            return;
        };
        let mut workspace_status_signatures = load_workspace_status_signatures(dispatcher).await;

        let mut receiver = deployment.events().msg_store().get_receiver();
        loop {
            match recv_next_workspace_event(&mut receiver).await {
                Some(LogMsg::JsonPatch(patch)) => {
                    for operation in patch.0 {
                        match operation {
                            PatchOperation::Add(add)
                                if add.path.to_string().starts_with("/workspaces/") =>
                            {
                                if let Ok(workspace) =
                                    serde_json::from_value::<WorkspaceWithStatus>(add.value)
                                    && should_dispatch_workspace_status(
                                        &mut workspace_status_signatures,
                                        &workspace.workspace,
                                    )
                                {
                                    let _ = dispatcher
                                        .dispatch_workspace_status(&workspace.workspace)
                                        .await;
                                }
                            }
                            PatchOperation::Add(add)
                                if add.path.to_string().starts_with("/execution_processes/") =>
                            {
                                if let Ok(process) =
                                    serde_json::from_value::<ExecutionProcess>(add.value)
                                {
                                    let dispatcher = dispatcher.clone();
                                    tokio::spawn(async move {
                                        let _ = dispatcher
                                            .dispatch_execution_process_reply(process.id)
                                            .await;
                                    });
                                }
                            }
                            PatchOperation::Add(add)
                                if add
                                    .path
                                    .to_string()
                                    .starts_with("/coding_agent_turn_summaries/") =>
                            {
                                if let Ok(summary_patch) =
                                    serde_json::from_value::<CodingAgentTurnSummaryPatch>(add.value)
                                {
                                    let dispatcher = dispatcher.clone();
                                    tokio::spawn(async move {
                                        let _ = dispatcher
                                            .dispatch_execution_process_reply(
                                                summary_patch.execution_process_id,
                                            )
                                            .await;
                                    });
                                }
                            }
                            PatchOperation::Replace(replace)
                                if replace.path.to_string().starts_with("/workspaces/") =>
                            {
                                if let Ok(workspace) =
                                    serde_json::from_value::<WorkspaceWithStatus>(replace.value)
                                    && should_dispatch_workspace_status(
                                        &mut workspace_status_signatures,
                                        &workspace.workspace,
                                    )
                                {
                                    let _ = dispatcher
                                        .dispatch_workspace_status(&workspace.workspace)
                                        .await;
                                }
                            }
                            PatchOperation::Replace(replace)
                                if replace
                                    .path
                                    .to_string()
                                    .starts_with("/execution_processes/") =>
                            {
                                if let Ok(process) =
                                    serde_json::from_value::<ExecutionProcess>(replace.value)
                                {
                                    let dispatcher = dispatcher.clone();
                                    tokio::spawn(async move {
                                        let _ = dispatcher
                                            .dispatch_execution_process_reply(process.id)
                                            .await;
                                    });
                                }
                            }
                            PatchOperation::Replace(replace)
                                if replace
                                    .path
                                    .to_string()
                                    .starts_with("/coding_agent_turn_summaries/") =>
                            {
                                if let Ok(summary_patch) =
                                    serde_json::from_value::<CodingAgentTurnSummaryPatch>(
                                        replace.value,
                                    )
                                {
                                    let dispatcher = dispatcher.clone();
                                    tokio::spawn(async move {
                                        let _ = dispatcher
                                            .dispatch_execution_process_reply(
                                                summary_patch.execution_process_id,
                                            )
                                            .await;
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Some(_) => {}
                None => break,
            }
        }
    })
}

async fn recv_next_workspace_event(
    receiver: &mut tokio::sync::broadcast::Receiver<LogMsg>,
) -> Option<LogMsg> {
    loop {
        match receiver.recv().await {
            Ok(message) => return Some(message),
            Err(RecvError::Lagged(skipped)) => {
                tracing::warn!(
                    skipped,
                    "Feishu workspace event bridge lagged; skipping stale messages"
                );
            }
            Err(RecvError::Closed) => {
                tracing::warn!("Feishu workspace event bridge stopped");
                return None;
            }
        }
    }
}

async fn load_workspace_status_signatures<S, C>(
    dispatcher: &FeishuDispatcher<S, C>,
) -> HashMap<Uuid, String>
where
    S: crate::feishu::secret_store::FeishuSecretStore + 'static,
    C: FeishuClient + 'static,
{
    Workspace::fetch_all(dispatcher.service.pool())
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|workspace| (workspace.id, workspace_status_signature(&workspace)))
        .collect()
}

fn should_dispatch_workspace_status(
    signatures: &mut HashMap<Uuid, String>,
    workspace: &Workspace,
) -> bool {
    let signature = workspace_status_signature(workspace);
    match signatures.insert(workspace.id, signature.clone()) {
        Some(previous) => previous != signature,
        None => false,
    }
}

fn workspace_status_signature(workspace: &Workspace) -> String {
    format!(
        "{}|{}|{}|{}",
        workspace.archived,
        workspace.pinned,
        workspace.branch,
        workspace.name.as_deref().unwrap_or_default()
    )
}

fn normalized_ack_reaction_emoji_type(value: &str) -> &str {
    if SUPPORTED_ACK_REACTION_EMOJI_TYPES.contains(&value) {
        value
    } else {
        DEFAULT_ACK_REACTION_EMOJI_TYPE
    }
}

fn format_group_announcement_content(
    workspace_with_status: &WorkspaceWithStatus,
    override_state: Option<FeishuAnnouncementState>,
) -> String {
    let status = match override_state.unwrap_or({
        if workspace_with_status.is_running {
            FeishuAnnouncementState::Running
        } else if workspace_with_status.is_errored {
            FeishuAnnouncementState::NeedsAttention
        } else {
            FeishuAnnouncementState::Idle
        }
    }) {
        FeishuAnnouncementState::Running => "运行中",
        FeishuAnnouncementState::Idle => "空闲",
        FeishuAnnouncementState::NeedsAttention => "需要注意",
    };

    format!(
        "Vibe Kanban | {} | 工作区：{} | 分支：{} | 已归档：{} | 已置顶：{}",
        status,
        workspace_with_status
            .workspace
            .name
            .as_deref()
            .unwrap_or(&workspace_with_status.workspace.branch),
        workspace_with_status.workspace.branch,
        if workspace_with_status.workspace.archived {
            "是"
        } else {
            "否"
        },
        if workspace_with_status.workspace.pinned {
            "是"
        } else {
            "否"
        }
    )
}

#[derive(Debug, serde::Deserialize)]
struct CodingAgentTurnSummaryPatch {
    execution_process_id: Uuid,
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, str::FromStr, sync::Arc};

    use anyhow::{Result, anyhow};
    use async_trait::async_trait;
    use db::models::{
        coding_agent_turn::{CodingAgentTurn, CreateCodingAgentTurn},
        execution_process::{
            CreateExecutionProcess, ExecutionProcess, ExecutionProcessRunReason,
            ExecutionProcessStatus,
        },
        feishu_bot_target::{CreateFeishuBotTarget, FeishuBotTarget},
        session::Session,
        workspace::{CreateWorkspace, Workspace},
        workspace_feishu_binding::{
            CreateWorkspaceFeishuBinding, UpdateWorkspaceFeishuBinding, WorkspaceFeishuBinding,
        },
    };
    use executors::{
        actions::{
            ExecutorAction, ExecutorActionType, coding_agent_initial::CodingAgentInitialRequest,
        },
        executors::BaseCodingAgent,
        profile::ExecutorConfig,
    };
    use secrecy::SecretString;
    use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
    use tokio::sync::{Mutex, broadcast};
    use utils::log_msg::LogMsg;
    use uuid::Uuid;

    use super::{FeishuDispatcher, recv_next_workspace_event};
    use crate::feishu::{
        chat_runner::FeishuChatRunner,
        client::FeishuClient,
        secret_store::FeishuSecretStore,
        service::FeishuService,
        types::{
            CreateFeishuBotInput, DiscoveredFeishuTarget, ResolvedFeishuSecrets,
            SendFeishuMessageResult,
        },
    };

    #[derive(Debug, Default)]
    struct MemoryFeishuSecretStore {
        values: Mutex<HashMap<String, SecretString>>,
    }

    #[async_trait]
    impl FeishuSecretStore for MemoryFeishuSecretStore {
        async fn put(&self, value: SecretString) -> Result<String> {
            let reference = format!("memory:{}", uuid::Uuid::new_v4());
            self.values.lock().await.insert(reference.clone(), value);
            Ok(reference)
        }

        async fn get(&self, reference: &str) -> Result<SecretString> {
            self.values
                .lock()
                .await
                .get(reference)
                .cloned()
                .ok_or_else(|| anyhow!("missing secret"))
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct SentMessage {
        target_id: uuid::Uuid,
        message: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct AddedReaction {
        message_id: String,
        emoji_type: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct AnnouncementUpdate {
        chat_id: String,
        content: String,
    }

    #[derive(Debug, Default)]
    struct RecordingFeishuClient {
        sent_messages: Mutex<Vec<SentMessage>>,
        added_reactions: Mutex<Vec<AddedReaction>>,
        announcement_updates: Mutex<Vec<AnnouncementUpdate>>,
    }

    #[derive(Debug, Clone)]
    struct SubmittedChatMessage {
        session_id: Uuid,
        workspace_id: Uuid,
        prompt: String,
    }

    #[derive(Debug, Default)]
    struct RecordingChatRunner {
        submissions: Mutex<Vec<SubmittedChatMessage>>,
    }

    #[async_trait]
    impl FeishuChatRunner for RecordingChatRunner {
        async fn submit_workspace_chat_message(
            &self,
            session_id: Uuid,
            workspace_id: Uuid,
            prompt: &str,
        ) -> Result<()> {
            self.submissions.lock().await.push(SubmittedChatMessage {
                session_id,
                workspace_id,
                prompt: prompt.to_string(),
            });
            Ok(())
        }
    }

    #[async_trait]
    impl FeishuClient for RecordingFeishuClient {
        async fn validate_bot(
            &self,
            _bot: &db::models::feishu_bot::FeishuBot,
            _secrets: &ResolvedFeishuSecrets,
        ) -> Result<Vec<DiscoveredFeishuTarget>> {
            Ok(vec![])
        }

        async fn discover_targets(
            &self,
            _bot: &db::models::feishu_bot::FeishuBot,
            _secrets: &ResolvedFeishuSecrets,
        ) -> Result<Vec<DiscoveredFeishuTarget>> {
            Ok(vec![])
        }

        async fn send_message(
            &self,
            _bot: &db::models::feishu_bot::FeishuBot,
            _secrets: &ResolvedFeishuSecrets,
            target: &FeishuBotTarget,
            message: &str,
        ) -> Result<SendFeishuMessageResult> {
            self.sent_messages.lock().await.push(SentMessage {
                target_id: target.id,
                message: message.to_string(),
            });
            Ok(SendFeishuMessageResult {
                message_id: Some("sent-from-test".into()),
            })
        }

        async fn add_message_reaction(
            &self,
            _bot: &db::models::feishu_bot::FeishuBot,
            _secrets: &ResolvedFeishuSecrets,
            message_id: &str,
            emoji_type: &str,
        ) -> Result<()> {
            self.added_reactions.lock().await.push(AddedReaction {
                message_id: message_id.to_string(),
                emoji_type: emoji_type.to_string(),
            });
            Ok(())
        }

        async fn upsert_status_announcement(
            &self,
            _bot: &db::models::feishu_bot::FeishuBot,
            _secrets: &ResolvedFeishuSecrets,
            chat_id: &str,
            content: &str,
        ) -> Result<()> {
            self.announcement_updates
                .lock()
                .await
                .push(AnnouncementUpdate {
                    chat_id: chat_id.to_string(),
                    content: content.to_string(),
                });
            Ok(())
        }
    }

    async fn test_pool() -> SqlitePool {
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .expect("memory sqlite url")
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("connect memory db");

        sqlx::migrate!("../db/migrations")
            .run(&pool)
            .await
            .expect("run migrations");

        pool
    }

    async fn create_workspace(pool: &SqlitePool, name: &str) -> Workspace {
        Workspace::create(
            pool,
            &CreateWorkspace {
                branch: format!("vk/{name}"),
                name: Some(name.to_string()),
            },
            uuid::Uuid::new_v4(),
        )
        .await
        .expect("create workspace")
    }

    async fn create_bot(
        service: &FeishuService<MemoryFeishuSecretStore>,
        name: &str,
    ) -> db::models::feishu_bot::FeishuBot {
        service
            .create_bot(CreateFeishuBotInput {
                name: name.to_string(),
                app_id: format!("cli_{name}"),
                app_secret: Some(SecretString::new("app-secret".into())),
                encrypt_key: None,
                verification_token: None,
                tenant_mode: "self_built".into(),
            })
            .await
            .expect("create bot")
    }

    async fn create_target(
        pool: &SqlitePool,
        bot_id: uuid::Uuid,
        chat_id: &str,
    ) -> FeishuBotTarget {
        FeishuBotTarget::create(
            pool,
            &CreateFeishuBotTarget {
                bot_id,
                target_type: "chat".into(),
                open_chat_id: None,
                chat_id: Some(chat_id.to_string()),
                name: format!("Target {chat_id}"),
                source: "test".into(),
            },
        )
        .await
        .expect("create target")
    }

    async fn enable_chat_mode(service: &FeishuService<MemoryFeishuSecretStore>, binding_id: Uuid) {
        service
            .update_binding(
                binding_id,
                UpdateWorkspaceFeishuBinding {
                    enabled: true,
                    notify_on_status: true,
                    notify_on_agent_reply: true,
                    notify_on_pr: true,
                    allow_commands: false,
                    allow_chat_messages: true,
                    allow_cards: true,
                    ack_reaction_enabled: true,
                    ack_reaction_emoji_type: "Typing".to_string(),
                    sync_group_announcement: true,
                },
            )
            .await
            .expect("enable chat mode");
    }

    async fn force_enable_chat_mode(pool: &SqlitePool, binding_id: Uuid) {
        WorkspaceFeishuBinding::update(
            pool,
            binding_id,
            &UpdateWorkspaceFeishuBinding {
                enabled: true,
                notify_on_status: true,
                notify_on_agent_reply: true,
                notify_on_pr: true,
                allow_commands: false,
                allow_chat_messages: true,
                allow_cards: true,
                ack_reaction_enabled: true,
                ack_reaction_emoji_type: "Typing".to_string(),
                sync_group_announcement: true,
            },
        )
        .await
        .expect("force enable legacy chat mode");
    }

    #[tokio::test]
    async fn dispatches_status_reply_for_bound_chat_commands() {
        let pool = test_pool().await;
        let service = Arc::new(FeishuService::new(
            pool.clone(),
            Arc::new(MemoryFeishuSecretStore::default()),
        ));
        let client = Arc::new(RecordingFeishuClient::default());
        let dispatcher = FeishuDispatcher::new(service.clone(), client.clone());

        let workspace = create_workspace(&pool, "Workspace A").await;
        let bot = create_bot(service.as_ref(), "bot-a").await;
        let target = create_target(&pool, bot.id, "chat_status").await;
        service
            .create_binding(workspace.id, bot.id, target.id)
            .await
            .expect("create binding");

        let response = dispatcher
            .dispatch_inbound_chat_message(
                "event-status",
                bot.id,
                Some("chat_status"),
                None,
                Some("om_status"),
                "/vk status",
            )
            .await
            .expect("dispatch inbound message");

        assert!(response.is_some());
        let sent_messages = client.sent_messages.lock().await.clone();
        assert_eq!(sent_messages.len(), 1);
        assert_eq!(sent_messages[0].target_id, target.id);
        assert!(sent_messages[0].message.contains("Workspace A"));
    }

    #[tokio::test]
    async fn routes_plain_chat_messages_into_fixed_workspace_session() {
        let pool = test_pool().await;
        let service = Arc::new(FeishuService::new(
            pool.clone(),
            Arc::new(MemoryFeishuSecretStore::default()),
        ));
        let client = Arc::new(RecordingFeishuClient::default());
        let chat_runner = Arc::new(RecordingChatRunner::default());
        let dispatcher = FeishuDispatcher::with_chat_runner(
            service.clone(),
            client.clone(),
            chat_runner.clone(),
        );

        let workspace = create_workspace(&pool, "Workspace B").await;
        let bot = create_bot(service.as_ref(), "bot-b").await;
        let target = create_target(&pool, bot.id, "chat_ignore").await;
        let binding = service
            .create_binding(workspace.id, bot.id, target.id)
            .await
            .expect("create binding");
        enable_chat_mode(service.as_ref(), binding.id).await;

        let response = dispatcher
            .dispatch_inbound_chat_message(
                "event-ignore",
                bot.id,
                Some("chat_ignore"),
                None,
                Some("om_ignore"),
                "hello from group",
            )
            .await
            .expect("dispatch inbound message");

        assert!(response.is_none());
        assert!(client.sent_messages.lock().await.is_empty());
        let submissions = chat_runner.submissions.lock().await.clone();
        assert_eq!(submissions.len(), 1);
        assert_eq!(submissions[0].workspace_id, workspace.id);
        assert_eq!(submissions[0].prompt, "hello from group");

        let conversations = service
            .list_conversations_for_session(submissions[0].session_id)
            .await
            .expect("list conversations");
        assert_eq!(conversations.len(), 1);
        assert_eq!(conversations[0].workspace_id, workspace.id);
    }

    #[tokio::test]
    async fn adds_receive_ack_reaction_for_inbound_chat_messages() {
        let pool = test_pool().await;
        let service = Arc::new(FeishuService::new(
            pool.clone(),
            Arc::new(MemoryFeishuSecretStore::default()),
        ));
        let client = Arc::new(RecordingFeishuClient::default());
        let chat_runner = Arc::new(RecordingChatRunner::default());
        let dispatcher = FeishuDispatcher::with_chat_runner(
            service.clone(),
            client.clone(),
            chat_runner.clone(),
        );

        let workspace = create_workspace(&pool, "Workspace Ack").await;
        let bot = create_bot(service.as_ref(), "bot-ack").await;
        let target = create_target(&pool, bot.id, "chat_ack").await;
        let binding = service
            .create_binding(workspace.id, bot.id, target.id)
            .await
            .expect("create binding");
        enable_chat_mode(service.as_ref(), binding.id).await;

        let response = dispatcher
            .dispatch_inbound_chat_message(
                "event-ack",
                bot.id,
                Some("chat_ack"),
                None,
                Some("om_ack_123"),
                "hello from group",
            )
            .await
            .expect("dispatch inbound message");

        assert!(response.is_none());
        assert_eq!(
            client.added_reactions.lock().await.clone(),
            vec![AddedReaction {
                message_id: "om_ack_123".into(),
                emoji_type: "Typing".into(),
            }]
        );
        assert_eq!(chat_runner.submissions.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn updates_group_announcement_for_binding_status() {
        let pool = test_pool().await;
        let service = Arc::new(FeishuService::new(
            pool.clone(),
            Arc::new(MemoryFeishuSecretStore::default()),
        ));
        let client = Arc::new(RecordingFeishuClient::default());
        let chat_runner = Arc::new(RecordingChatRunner::default());
        let dispatcher = FeishuDispatcher::with_chat_runner(
            service.clone(),
            client.clone(),
            chat_runner.clone(),
        );

        let workspace = create_workspace(&pool, "Workspace Announcement").await;
        let bot = create_bot(service.as_ref(), "bot-announcement").await;
        let target = create_target(&pool, bot.id, "chat_announcement").await;
        let binding = service
            .create_binding(workspace.id, bot.id, target.id)
            .await
            .expect("create binding");
        enable_chat_mode(service.as_ref(), binding.id).await;

        dispatcher
            .dispatch_inbound_chat_message(
                "event-announcement",
                bot.id,
                Some("chat_announcement"),
                None,
                Some("om_announcement_123"),
                "ship it",
            )
            .await
            .expect("dispatch inbound message");

        let announcement_updates = client.announcement_updates.lock().await.clone();
        assert_eq!(announcement_updates.len(), 1);
        assert_eq!(announcement_updates[0].chat_id, "chat_announcement");
        assert!(announcement_updates[0].content.contains("运行中"));
        assert!(announcement_updates[0].content.contains(&workspace.branch));

        let session_id = chat_runner.submissions.lock().await[0].session_id;
        let session = Session::find_by_id(&pool, session_id)
            .await
            .expect("find session")
            .expect("session exists");
        let action = ExecutorAction::new(
            ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
                prompt: "ship it".into(),
                executor_config: ExecutorConfig::new(BaseCodingAgent::ClaudeCode),
                working_dir: None,
            }),
            None,
        );

        let process = ExecutionProcess::create(
            &pool,
            &CreateExecutionProcess {
                session_id: session.id,
                executor_action: action,
                run_reason: ExecutionProcessRunReason::CodingAgent,
            },
            Uuid::new_v4(),
            &[],
        )
        .await
        .expect("create execution process");
        CodingAgentTurn::create(
            &pool,
            &CreateCodingAgentTurn {
                execution_process_id: process.id,
                prompt: Some("ship it".into()),
            },
            Uuid::new_v4(),
        )
        .await
        .expect("create turn");
        CodingAgentTurn::update_summary(&pool, process.id, "Done")
            .await
            .expect("update summary");
        ExecutionProcess::update_completion(
            &pool,
            process.id,
            ExecutionProcessStatus::Completed,
            Some(0),
        )
        .await
        .expect("complete process");

        dispatcher
            .dispatch_execution_process_reply(process.id)
            .await
            .expect("dispatch reply");

        let announcement_updates = client.announcement_updates.lock().await.clone();
        assert_eq!(announcement_updates.len(), 2);
        assert!(announcement_updates[1].content.contains("空闲"));
    }

    #[tokio::test]
    async fn reuses_same_fixed_session_for_follow_up_messages() {
        let pool = test_pool().await;
        let service = Arc::new(FeishuService::new(
            pool.clone(),
            Arc::new(MemoryFeishuSecretStore::default()),
        ));
        let client = Arc::new(RecordingFeishuClient::default());
        let chat_runner = Arc::new(RecordingChatRunner::default());
        let dispatcher = FeishuDispatcher::with_chat_runner(
            service.clone(),
            client.clone(),
            chat_runner.clone(),
        );

        let workspace = create_workspace(&pool, "Workspace Reuse").await;
        let bot = create_bot(service.as_ref(), "bot-reuse").await;
        let target = create_target(&pool, bot.id, "chat_reuse").await;
        let binding = service
            .create_binding(workspace.id, bot.id, target.id)
            .await
            .expect("create binding");
        enable_chat_mode(service.as_ref(), binding.id).await;

        dispatcher
            .dispatch_inbound_chat_message(
                "event-reuse-1",
                bot.id,
                Some("chat_reuse"),
                None,
                Some("om_reuse_1"),
                "first",
            )
            .await
            .expect("first dispatch");
        dispatcher
            .dispatch_inbound_chat_message(
                "event-reuse-2",
                bot.id,
                Some("chat_reuse"),
                None,
                Some("om_reuse_2"),
                "second",
            )
            .await
            .expect("second dispatch");

        let submissions = chat_runner.submissions.lock().await.clone();
        assert_eq!(submissions.len(), 2);
        assert_eq!(submissions[0].session_id, submissions[1].session_id);
    }

    #[tokio::test]
    async fn ignores_duplicate_inbound_message_ids_from_feishu() {
        let pool = test_pool().await;
        let service = Arc::new(FeishuService::new(
            pool.clone(),
            Arc::new(MemoryFeishuSecretStore::default()),
        ));
        let client = Arc::new(RecordingFeishuClient::default());
        let chat_runner = Arc::new(RecordingChatRunner::default());
        let dispatcher = FeishuDispatcher::with_chat_runner(
            service.clone(),
            client.clone(),
            chat_runner.clone(),
        );

        let workspace = create_workspace(&pool, "Workspace Dedup").await;
        let bot = create_bot(service.as_ref(), "bot-dedup").await;
        let target = create_target(&pool, bot.id, "chat_dedup").await;
        let binding = service
            .create_binding(workspace.id, bot.id, target.id)
            .await
            .expect("create binding");
        enable_chat_mode(service.as_ref(), binding.id).await;

        dispatcher
            .dispatch_inbound_chat_message(
                "event-dedup-1",
                bot.id,
                Some("chat_dedup"),
                None,
                Some("om_dedup_123"),
                "same incoming text",
            )
            .await
            .expect("first dispatch");

        dispatcher
            .dispatch_inbound_chat_message(
                "event-dedup-2",
                bot.id,
                Some("chat_dedup"),
                None,
                Some("om_dedup_123"),
                "same incoming text",
            )
            .await
            .expect("duplicate dispatch");

        assert_eq!(chat_runner.submissions.lock().await.len(), 1);
        assert_eq!(client.added_reactions.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn ignores_plain_chat_messages_when_chat_mode_disabled() {
        let pool = test_pool().await;
        let service = Arc::new(FeishuService::new(
            pool.clone(),
            Arc::new(MemoryFeishuSecretStore::default()),
        ));
        let client = Arc::new(RecordingFeishuClient::default());
        let chat_runner = Arc::new(RecordingChatRunner::default());
        let dispatcher = FeishuDispatcher::with_chat_runner(
            service.clone(),
            client.clone(),
            chat_runner.clone(),
        );

        let workspace = create_workspace(&pool, "Workspace Off").await;
        let bot = create_bot(service.as_ref(), "bot-off").await;
        let target = create_target(&pool, bot.id, "chat_off").await;
        service
            .create_binding(workspace.id, bot.id, target.id)
            .await
            .expect("create binding");

        let response = dispatcher
            .dispatch_inbound_chat_message(
                "event-off",
                bot.id,
                Some("chat_off"),
                None,
                Some("om_off"),
                "hello",
            )
            .await
            .expect("dispatch inbound message");

        assert!(response.is_none());
        assert!(client.sent_messages.lock().await.is_empty());
        assert!(chat_runner.submissions.lock().await.is_empty());
    }

    #[tokio::test]
    async fn reconciles_legacy_duplicate_bindings_for_one_chat_target() {
        let pool = test_pool().await;
        let service = Arc::new(FeishuService::new(
            pool.clone(),
            Arc::new(MemoryFeishuSecretStore::default()),
        ));
        let client = Arc::new(RecordingFeishuClient::default());
        let chat_runner = Arc::new(RecordingChatRunner::default());
        let dispatcher = FeishuDispatcher::with_chat_runner(
            service.clone(),
            client.clone(),
            chat_runner.clone(),
        );

        let first = create_workspace(&pool, "Workspace One").await;
        let second = create_workspace(&pool, "Workspace Two").await;
        let bot = create_bot(service.as_ref(), "bot-c").await;
        let target = create_target(&pool, bot.id, "chat_shared").await;
        let first_binding = WorkspaceFeishuBinding::create(
            &pool,
            &CreateWorkspaceFeishuBinding {
                workspace_id: first.id,
                bot_id: bot.id,
                target_id: target.id,
            },
        )
        .await
        .expect("create first binding");
        let second_binding = WorkspaceFeishuBinding::create(
            &pool,
            &CreateWorkspaceFeishuBinding {
                workspace_id: second.id,
                bot_id: bot.id,
                target_id: target.id,
            },
        )
        .await
        .expect("create second binding");
        force_enable_chat_mode(&pool, first_binding.id).await;
        force_enable_chat_mode(&pool, second_binding.id).await;

        let response = dispatcher
            .dispatch_inbound_chat_message(
                "event-shared",
                bot.id,
                Some("chat_shared"),
                None,
                Some("om_shared"),
                "hello from shared chat",
            )
            .await
            .expect("dispatch inbound message");

        assert!(response.is_none());
        let submissions = chat_runner.submissions.lock().await.clone();
        assert_eq!(submissions.len(), 1);
        assert_eq!(submissions[0].workspace_id, second.id);

        let first_binding = WorkspaceFeishuBinding::find_by_id(&pool, first_binding.id)
            .await
            .expect("reload first binding")
            .expect("first binding exists");
        let second_binding = WorkspaceFeishuBinding::find_by_id(&pool, second_binding.id)
            .await
            .expect("reload second binding")
            .expect("second binding exists");
        assert!(!first_binding.enabled);
        assert!(second_binding.enabled);
    }

    #[tokio::test]
    async fn dispatches_execution_replies_back_to_bound_feishu_conversation() {
        let pool = test_pool().await;
        let service = Arc::new(FeishuService::new(
            pool.clone(),
            Arc::new(MemoryFeishuSecretStore::default()),
        ));
        let client = Arc::new(RecordingFeishuClient::default());
        let dispatcher = FeishuDispatcher::new(service.clone(), client.clone());

        let workspace = create_workspace(&pool, "Workspace Reply").await;
        let bot = create_bot(service.as_ref(), "bot-reply").await;
        let target = create_target(&pool, bot.id, "chat_reply").await;
        let binding = service
            .create_binding(workspace.id, bot.id, target.id)
            .await
            .expect("create binding");
        enable_chat_mode(service.as_ref(), binding.id).await;

        let conversation = service
            .ensure_chat_session_for_binding(&binding)
            .await
            .expect("ensure conversation");
        let session_id = conversation.session_id.expect("session id");
        let session = Session::find_by_id(&pool, session_id)
            .await
            .expect("find session")
            .expect("session exists");

        let action = ExecutorAction::new(
            ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
                prompt: "hello".into(),
                executor_config: ExecutorConfig::new(BaseCodingAgent::ClaudeCode),
                working_dir: None,
            }),
            None,
        );

        let process = ExecutionProcess::create(
            &pool,
            &CreateExecutionProcess {
                session_id: session.id,
                executor_action: action,
                run_reason: ExecutionProcessRunReason::CodingAgent,
            },
            Uuid::new_v4(),
            &[],
        )
        .await
        .expect("create execution process");
        CodingAgentTurn::create(
            &pool,
            &CreateCodingAgentTurn {
                execution_process_id: process.id,
                prompt: Some("hello".into()),
            },
            Uuid::new_v4(),
        )
        .await
        .expect("create turn");
        CodingAgentTurn::update_summary(&pool, process.id, "Agent reply from session")
            .await
            .expect("update summary");
        ExecutionProcess::update_completion(
            &pool,
            process.id,
            ExecutionProcessStatus::Completed,
            Some(0),
        )
        .await
        .expect("complete process");

        let sent = dispatcher
            .dispatch_execution_process_reply(process.id)
            .await
            .expect("dispatch reply");

        assert_eq!(sent, 1);
        let sent_messages = client.sent_messages.lock().await.clone();
        assert_eq!(sent_messages.len(), 1);
        assert_eq!(sent_messages[0].target_id, target.id);
        assert_eq!(sent_messages[0].message, "Agent reply from session");
    }

    #[tokio::test]
    async fn running_process_probe_does_not_block_later_completion_reply() {
        let pool = test_pool().await;
        let service = Arc::new(FeishuService::new(
            pool.clone(),
            Arc::new(MemoryFeishuSecretStore::default()),
        ));
        let client = Arc::new(RecordingFeishuClient::default());
        let dispatcher = FeishuDispatcher::new(service.clone(), client.clone());

        let workspace = create_workspace(&pool, "Workspace Probe").await;
        let bot = create_bot(service.as_ref(), "bot-probe").await;
        let target = create_target(&pool, bot.id, "chat_probe").await;
        let binding = service
            .create_binding(workspace.id, bot.id, target.id)
            .await
            .expect("create binding");
        enable_chat_mode(service.as_ref(), binding.id).await;

        let conversation = service
            .ensure_chat_session_for_binding(&binding)
            .await
            .expect("ensure conversation");
        let session_id = conversation.session_id.expect("session id");
        let session = Session::find_by_id(&pool, session_id)
            .await
            .expect("find session")
            .expect("session exists");

        let action = ExecutorAction::new(
            ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
                prompt: "hello".into(),
                executor_config: ExecutorConfig::new(BaseCodingAgent::ClaudeCode),
                working_dir: None,
            }),
            None,
        );

        let process = ExecutionProcess::create(
            &pool,
            &CreateExecutionProcess {
                session_id: session.id,
                executor_action: action,
                run_reason: ExecutionProcessRunReason::CodingAgent,
            },
            Uuid::new_v4(),
            &[],
        )
        .await
        .expect("create execution process");

        let early = dispatcher
            .dispatch_execution_process_reply(process.id)
            .await
            .expect("running probe");
        assert_eq!(early, 0);

        CodingAgentTurn::create(
            &pool,
            &CreateCodingAgentTurn {
                execution_process_id: process.id,
                prompt: Some("hello".into()),
            },
            Uuid::new_v4(),
        )
        .await
        .expect("create turn");
        CodingAgentTurn::update_summary(&pool, process.id, "Reply after completion")
            .await
            .expect("update summary");
        ExecutionProcess::update_completion(
            &pool,
            process.id,
            ExecutionProcessStatus::Completed,
            Some(0),
        )
        .await
        .expect("complete process");

        let sent = dispatcher
            .dispatch_execution_process_reply(process.id)
            .await
            .expect("completion reply");

        assert_eq!(sent, 1);
        let sent_messages = client.sent_messages.lock().await.clone();
        assert_eq!(sent_messages.len(), 1);
        assert_eq!(sent_messages[0].message, "Reply after completion");
    }

    #[tokio::test]
    async fn completed_process_without_summary_can_reply_after_late_summary() {
        let pool = test_pool().await;
        let service = Arc::new(FeishuService::new(
            pool.clone(),
            Arc::new(MemoryFeishuSecretStore::default()),
        ));
        let client = Arc::new(RecordingFeishuClient::default());
        let dispatcher = FeishuDispatcher::new(service.clone(), client.clone());

        let workspace = create_workspace(&pool, "Workspace Late Summary").await;
        let bot = create_bot(service.as_ref(), "bot-late-summary").await;
        let target = create_target(&pool, bot.id, "chat_late_summary").await;
        let binding = service
            .create_binding(workspace.id, bot.id, target.id)
            .await
            .expect("create binding");
        enable_chat_mode(service.as_ref(), binding.id).await;

        let conversation = service
            .ensure_chat_session_for_binding(&binding)
            .await
            .expect("ensure conversation");
        let session_id = conversation.session_id.expect("session id");
        let session = Session::find_by_id(&pool, session_id)
            .await
            .expect("find session")
            .expect("session exists");

        let action = ExecutorAction::new(
            ExecutorActionType::CodingAgentInitialRequest(CodingAgentInitialRequest {
                prompt: "hello".into(),
                executor_config: ExecutorConfig::new(BaseCodingAgent::ClaudeCode),
                working_dir: None,
            }),
            None,
        );

        let process = ExecutionProcess::create(
            &pool,
            &CreateExecutionProcess {
                session_id: session.id,
                executor_action: action,
                run_reason: ExecutionProcessRunReason::CodingAgent,
            },
            Uuid::new_v4(),
            &[],
        )
        .await
        .expect("create execution process");
        CodingAgentTurn::create(
            &pool,
            &CreateCodingAgentTurn {
                execution_process_id: process.id,
                prompt: Some("hello".into()),
            },
            Uuid::new_v4(),
        )
        .await
        .expect("create turn");
        ExecutionProcess::update_completion(
            &pool,
            process.id,
            ExecutionProcessStatus::Completed,
            Some(0),
        )
        .await
        .expect("complete process");

        let early = dispatcher
            .dispatch_execution_process_reply(process.id)
            .await
            .expect("early completed reply");
        assert_eq!(early, 0);
        assert!(client.sent_messages.lock().await.is_empty());

        CodingAgentTurn::update_summary(&pool, process.id, "Reply after late summary")
            .await
            .expect("update summary");

        let sent = dispatcher
            .dispatch_execution_process_reply(process.id)
            .await
            .expect("late summary reply");

        assert_eq!(sent, 1);
        let sent_messages = client.sent_messages.lock().await.clone();
        assert_eq!(sent_messages.len(), 1);
        assert_eq!(sent_messages[0].message, "Reply after late summary");
    }

    #[tokio::test]
    async fn workspace_event_bridge_keeps_running_after_broadcast_lag() {
        let (sender, mut receiver) = broadcast::channel(1);

        sender
            .send(LogMsg::Stdout("first".to_string()))
            .expect("send first");
        sender
            .send(LogMsg::Stdout("second".to_string()))
            .expect("send second");

        let next = recv_next_workspace_event(&mut receiver).await;

        assert!(matches!(next, Some(LogMsg::Stdout(message)) if message == "second"));
    }
}
