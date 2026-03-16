use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Image {
        mime_type: String,
        data_base64: String,
    },
    FileContent {
        file_name: Option<String>,
        media_type: Option<String>,
        text: String,
    },
}

impl ContentBlock {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } | Self::FileContent { text, .. } => Some(text),
            Self::Image { .. } => None,
        }
    }

    pub fn explicit_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            Self::Image { .. } | Self::FileContent { .. } => None,
        }
    }

    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image { .. })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    pub role: MessageRole,
    pub content: Vec<ContentBlock>,
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system_text(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: vec![ContentBlock::text(text)],
            tool_call_id: None,
        }
    }

    pub fn user(content: Vec<ContentBlock>) -> Self {
        Self {
            role: MessageRole::User,
            content,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: Vec<ContentBlock>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content,
            tool_call_id: None,
        }
    }

    pub fn tool(call_id: impl Into<String>, content: Vec<ContentBlock>) -> Self {
        Self {
            role: MessageRole::Tool,
            content,
            tool_call_id: Some(call_id.into()),
        }
    }
}
