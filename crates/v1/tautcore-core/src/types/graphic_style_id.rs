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

    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "standard" | "default" => Some(Self::Standard),
            "excalidraw" | "hand-drawn" | "handdrawn" => Some(Self::Excalidraw),
            "cross-hatch" | "crosshatch" => Some(Self::CrossHatch),
            "blueprint" | "blue-print" => Some(Self::Blueprint),
            "spatial-clarity" | "spatial" => Some(Self::SpatialClarity),
            "neon-glow" | "neon" => Some(Self::NeonGlow),
            "stipple" => Some(Self::Stipple),
            _ => None,
        }
    }
}

impl std::str::FromStr for GraphicStyleId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str(s).ok_or_else(|| format!("unknown graphic style: '{s}'"))
    }
}
