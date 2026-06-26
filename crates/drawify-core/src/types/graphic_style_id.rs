#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicStyleId {
    Standard,
    Excalidraw,
    CrossHatch,
    Blueprint,
    SpatialClarity,
    NeonGlow,
    Stipple,
}

impl GraphicStyleId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Excalidraw => "excalidraw",
            Self::CrossHatch => "cross-hatch",
            Self::Blueprint => "blueprint",
            Self::SpatialClarity => "spatial-clarity",
            Self::NeonGlow => "neon-glow",
            Self::Stipple => "stipple",
        }
    }
}
