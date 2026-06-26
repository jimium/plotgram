//! MindmapTree：规范化中间层，供三种 interchange 编码器消费。

/// 根标题与根节点标签的处理模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RootTitleMode {
    /// title 与 root.label 分别输出（默认）
    #[default]
    Separate,
    /// 合并为一个根文本 = title.unwrap_or(root.label)
    Merge,
}

/// 规范化后的 mindmap 树，供 outline / opml / freemind 编码器消费。
/// Export 与 Import 共用此结构（统一 owned 版本，避免生命周期泛型传染）。
#[derive(Debug, Clone)]
pub struct MindmapTree {
    /// 图表标题（来自 diagram title 属性；可为 None）
    pub title: Option<String>,
    /// 树形根节点
    pub root: MindmapTreeNode,
    /// 无法挂入树的孤立节点（宽松模式下非空）
    pub orphans: Vec<MindmapTreeNode>,
}

/// 规范化后的 mindmap 树节点。
#[derive(Debug, Clone)]
pub struct MindmapTreeNode {
    pub entity_id: String,
    pub label: String,
    pub entity_type: Option<String>,
    pub branch_slot: Option<usize>,
    pub tree_depth: Option<usize>,
    /// 按 relation 插入序排列的子节点
    pub children: Vec<MindmapTreeNode>,
}

/// 树合法性验证错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeValidationError {
    /// 节点有多个父 relation
    MultiParent { entity_id: String, parent_count: usize },
    /// 检测到环
    Cycle { entity_id: String },
    /// 存在不可达 root 的节点
    Unreachable { entity_ids: Vec<String> },
    /// 无 root entity
    NoRoot,
}

/// 树合法性检查选项。
#[derive(Debug, Clone)]
pub struct TreeValidationOptions {
    /// strict 模式：任何违规都报错（默认 true）
    pub strict: bool,
}

impl Default for TreeValidationOptions {
    fn default() -> Self {
        Self { strict: true }
    }
}

/// 树合法性检查结果。
#[derive(Debug, Clone)]
pub struct TreeValidationResult {
    /// 严格模式下发现的错误
    pub errors: Vec<TreeValidationError>,
    /// 宽松模式下的警告（多父取第一条、环截断、孤立节点）
    pub warnings: Vec<TreeValidationError>,
    /// 宽松模式下截断后的孤立节点 id
    pub orphan_ids: Vec<String>,
}

impl MindmapTreeNode {
    pub fn new(entity_id: String, label: String) -> Self {
        Self {
            entity_id,
            label,
            entity_type: None,
            branch_slot: None,
            tree_depth: None,
            children: Vec::new(),
        }
    }

    /// 递归统计节点总数（含自身）。
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(|c| c.count()).sum::<usize>()
    }
}

impl MindmapTree {
    /// 统计树中节点总数（root + orphans）。
    pub fn total_node_count(&self) -> usize {
        self.root.count() + self.orphans.iter().map(|o| o.count()).sum::<usize>()
    }
}
