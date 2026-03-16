use crate::domain::content::ContentBlock;
use crate::domain::content::Message;
use crate::domain::error::AgentError;
use crate::domain::error::AgentErrorCode;
use crate::domain::ledger::LedgerEvent;
use crate::domain::ledger::MessageStatus;
use crate::domain::ledger::SessionEventPayload;
use crate::domain::ledger::SessionLedger;
use crate::domain::session::CompactionMode;
use crate::domain::session::ContextState;
use crate::domain::session::VisibleMessage;
use crate::domain::turn::RequestContext;
use crate::domain::turn::TurnAugmentation;

pub const COMPACTION_SUMMARY_PREFIX: &str = "[compacted history summary]";
pub const INTERRUPTED_MESSAGE_NOTICE: &str = "[此消息因用户取消而中断]";

pub struct ContextManager;

impl ContextManager {
    pub fn rebuild_visible_messages(ledger: &SessionLedger) -> ContextState {
        let latest_compaction = ledger.latest_compaction();
        let mut visible_messages = Vec::new();

        if let Some(record) = latest_compaction.clone() {
            match record.mode {
                CompactionMode::Summary => {
                    if let Some(summary_body) = record.summary_body.as_deref() {
                        visible_messages.push(VisibleMessage {
                            source_seq: None,
                            message: Message::system_text(Self::render_compaction_summary(
                                summary_body,
                            )),
                        });
                    }
                }
                CompactionMode::TruncationFallback => {}
            }
        }

        for event in ledger.events() {
            if latest_compaction
                .as_ref()
                .is_some_and(|record| event.seq <= record.replaces_through_seq)
            {
                continue;
            }
            visible_messages.extend(Self::visible_messages_for_event(event));
        }

        let history_estimated_tokens = visible_messages
            .iter()
            .map(|message| Self::estimate_message_tokens(&message.message))
            .sum();

        ContextState {
            latest_compaction,
            visible_messages,
            history_estimated_tokens,
        }
    }

    pub fn build_request_context(
        context_state: &ContextState,
        augmentation: &TurnAugmentation,
    ) -> Result<RequestContext, AgentError> {
        let anchor_index = context_state
            .visible_messages
            .iter()
            .position(|message| message.source_seq == Some(augmentation.origin_user_seq))
            .ok_or_else(|| {
                AgentError::new(
                    AgentErrorCode::InvalidInput,
                    "turn anchor message is no longer visible in the projected context",
                )
            })?;

        let pre_anchor_messages = context_state.visible_messages[..anchor_index]
            .iter()
            .map(|message| message.message.clone())
            .collect::<Vec<_>>();
        let anchor_message = context_state.visible_messages[anchor_index].message.clone();
        let post_anchor_messages = context_state.visible_messages[anchor_index + 1..]
            .iter()
            .map(|message| message.message.clone())
            .collect::<Vec<_>>();
        let post_anchor_augmentations = augmentation
            .skill_injections
            .iter()
            .map(|skill| Message::system_text(skill.rendered_xml.clone()))
            .collect::<Vec<_>>();

        Ok(RequestContext {
            pre_anchor_messages,
            anchor_message,
            post_anchor_augmentations,
            post_anchor_messages,
        })
    }

    pub fn render_compaction_summary(summary_body: &str) -> String {
        format!("{COMPACTION_SUMMARY_PREFIX}\n\n{summary_body}")
    }

    fn visible_messages_for_event(event: &LedgerEvent) -> Vec<VisibleMessage> {
        match &event.payload {
            SessionEventPayload::UserMessage { content } => vec![VisibleMessage {
                source_seq: Some(event.seq),
                message: Message::user(content.clone()),
            }],
            SessionEventPayload::AssistantMessage { content, status } => {
                let mut messages = vec![VisibleMessage {
                    source_seq: Some(event.seq),
                    message: Message::assistant(content.clone()),
                }];
                if *status == MessageStatus::Incomplete {
                    messages.push(VisibleMessage {
                        source_seq: None,
                        message: Message::system_text(INTERRUPTED_MESSAGE_NOTICE),
                    });
                }
                messages
            }
            SessionEventPayload::ToolResult { result } => vec![VisibleMessage {
                source_seq: Some(event.seq),
                message: Message::tool(
                    result.call_id.clone(),
                    vec![ContentBlock::text(result.output.content.clone())],
                ),
            }],
            SessionEventPayload::SystemMessage { content } => vec![VisibleMessage {
                source_seq: Some(event.seq),
                message: Message::system_text(content.clone()),
            }],
            SessionEventPayload::ToolCall { .. } | SessionEventPayload::Metadata { .. } => {
                Vec::new()
            }
        }
    }

    fn estimate_message_tokens(message: &Message) -> usize {
        message
            .content
            .iter()
            .map(|block| match block {
                ContentBlock::Text { text } | ContentBlock::FileContent { text, .. } => {
                    (text.len() / 4).max(1)
                }
                ContentBlock::Image { .. } => 256,
            })
            .sum()
    }
}
