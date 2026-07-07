use crate::types::GraphicStyleId;
use super::GraphicStylePainter;

pub struct StandardGraphicStylePainter;

impl GraphicStylePainter for StandardGraphicStylePainter {
    fn id(&self) -> GraphicStyleId {
        GraphicStyleId::Standard
    }
}

pub fn standard_marker_defs(active_stroke: &str, passive_stroke: &str) -> String {
    format!(
        r##"  <marker id="arrow-active" viewBox="0 0 10 10" refX="10" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
    <path d="M 0 0 L 10 5 L 0 10 z" fill="{active_stroke}"/>
  </marker>
  <marker id="arrow-passive" viewBox="0 0 10 10" refX="10" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
    <path d="M 0 0 L 10 5 L 0 10 z" fill="{passive_stroke}"/>
  </marker>
  <marker id="arrow-bidi" viewBox="0 0 10 10" refX="10" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
    <path d="M 0 0 L 10 5 L 0 10 z" fill="{active_stroke}"/>
  </marker>"##
    )
}
