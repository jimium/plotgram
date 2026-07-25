//! 问题节点推开与 momentum 抑制震荡。

use std::collections::HashMap;

/// 节点推动 momentum 历史，检测方向反转并施加衰减以抑制震荡。
#[derive(Default)]
pub struct MomentumHistory {
    prev_direction: HashMap<String, (f64, f64)>,
    pub reversal_count: usize,
}

impl MomentumHistory {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)] // 保留供测试与未来 push 恢复使用
    pub(crate) fn damp(&mut self, node_id: &str, fx: f64, fy: f64) -> (f64, f64) {
        if let Some(&(px, py)) = self.prev_direction.get(node_id) {
            let dot = fx * px + fy * py;
            if dot < 0.0 {
                self.reversal_count += 1;
                return (fx * 0.5, fy * 0.5);
            }
        }
        (fx, fy)
    }

    #[allow(dead_code)] // 保留供测试与未来 push 恢复使用
    pub(crate) fn update(&mut self, node_id: &str, fx: f64, fy: f64) {
        let len = (fx * fx + fy * fy).sqrt();
        if len > f64::EPSILON {
            self.prev_direction
                .insert(node_id.to_string(), (fx / len, fy / len));
        }
    }
}
