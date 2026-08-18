#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderOutput {
    Text(String),
    Binary(Vec<u8>),
}

impl RenderOutput {
    pub fn into_text(self) -> Option<String> {
        match self {
            Self::Text(text) => Some(text),
            Self::Binary(_) => None,
        }
    }

    pub fn into_bytes(self) -> Option<Vec<u8>> {
        match self {
            Self::Binary(bytes) => Some(bytes),
            Self::Text(_) => None,
        }
    }
}
