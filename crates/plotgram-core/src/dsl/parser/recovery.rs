//! 错误恢复：skip_to_next_statement 等

use crate::dsl::lexer::TokenKind;

use super::Parser;

impl Parser {
    // ── Error recovery ───────────────────────────────────

    /// 智能跳过到下一个语句的开始
    ///
    /// **策略**：
    /// 1. 跳过当前错误位置的所有 token
    /// 2. 找到下一个合法的语句起始点：
    ///    - `entity` 关键字
    ///    - `group` 关键字
    ///    - identifier（可能是 relation）
    ///    - `}`（表示当前 block 结束）
    /// 3. 如果遇到嵌套的 `{`，跳过整个 block
    ///
    /// 这样可以确保：
    /// - 即使当前 entity 解析失败，下一个 entity 还能被解析
    /// - 即使当前 group 解析失败，下一个 group 还能被解析
    /// - 即使当前 relation 解析失败，下一个 relation 还能被解析
    pub(super) fn skip_to_next_statement(&mut self) {
        let mut brace_depth = 0;

        while !self.at_eof() {
            match self.peek_kind() {
                // 遇到新的语句起始点，停止跳过
                TokenKind::Entity | TokenKind::Group | TokenKind::NodeStyle | TokenKind::EdgeStyle => {
                    if brace_depth == 0 {
                        return;
                    }
                    // 如果在 block 内，继续跳过
                    self.advance();
                }

                // 遇到 identifier，可能是 relation
                TokenKind::Ident(_) => {
                    if brace_depth == 0 {
                        // 检查是否是 diagram attribute（后面跟着 ':'）
                        if self.pos + 1 < self.tokens.len()
                            && matches!(self.tokens[self.pos + 1].kind, TokenKind::Colon) {
                            // 这是 diagram attribute，跳过
                            self.advance();
                        } else {
                            // 这可能是 relation，停止跳过
                            return;
                        }
                    } else {
                        // 在 block 内，继续跳过
                        self.advance();
                    }
                }

                // 遇到 '{'，增加嵌套深度
                TokenKind::LBrace => {
                    brace_depth += 1;
                    self.advance();
                }

                // 遇到 '}'，减少嵌套深度
                TokenKind::RBrace => {
                    if brace_depth == 0 {
                        // 回到当前 block 的结束点，停止跳过
                        return;
                    }
                    brace_depth -= 1;
                    self.advance();
                }

                // 其他 token，继续跳过
                _ => {
                    self.advance();
                }
            }
        }
    }
}
