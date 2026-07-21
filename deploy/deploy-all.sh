#!/usr/bin/env bash
# 一键全量发布：wasm → website → playground → showcase → agent-demo → agent-api
#
# 顺序依赖：
#   - wasm 必须在 playground / agent-demo 之前（它们 build 时需要本地 wasm 副本）
#   - 其他站点之间相互独立
#
# 用法:
#   ./deploy/deploy-all.sh                  # 全量发布（含 showcase SVG 渲染）
#   ./deploy/deploy-all.sh --skip-render    # 跳过 showcase SVG 渲染
#   ./deploy/deploy-all.sh --skip-api       # 跳过 agent-api（不编译 Rust 服务端）
#   ./deploy/deploy-all.sh --only wasm,agent-demo  # 只发布指定站点
#
# 环境变量：与各子脚本相同（DEPLOY_HOST / ASSET_HOST 等）

set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/common.sh"

SKIP_RENDER=false
SKIP_API=false
ONLY=""

usage() {
  cat <<'EOF'
用法: deploy/deploy-all.sh [选项]

按依赖顺序调用各站点发布脚本，全量发布。

顺序: wasm → website → playground → showcase → agent-demo → agent-api

选项:
  --skip-render        跳过 showcase SVG 渲染（传给 deploy-showcase.sh）
  --skip-api           跳过 agent-api 发布（不编译 Rust 服务端）
  --only LIST          只发布指定站点（逗号分隔，如 wasm,agent-demo）
                       可选: wasm website playground showcase agent-demo agent-api
  -h, --help           显示此帮助
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-render) SKIP_RENDER=true; shift ;;
    --skip-api)    SKIP_API=true; shift ;;
    --only)        ONLY="$2"; shift 2 ;;
    -h|--help)     usage; exit 0 ;;
    *)             echo "未知选项: $1" >&2; usage >&2; exit 1 ;;
  esac
done

# 默认全量发布顺序
ALL_STEPS=(wasm website playground showcase agent-demo agent-api)

# 解析 --only 过滤
STEPS=()
if [[ -n "$ONLY" ]]; then
  IFS=',' read -ra requested <<<"$ONLY"
  for r in "${requested[@]}"; do
    # 去除前后空格
    r="${r## }"; r="${r%% }"
    valid=false
    for s in "${ALL_STEPS[@]}"; do
      if [[ "$r" == "$s" ]]; then valid=true; break; fi
    done
    [[ "$valid" == true ]] || die "未知站点: $r（可选: ${ALL_STEPS[*]}）"
    STEPS+=("$r")
  done
else
  STEPS=("${ALL_STEPS[@]}")
fi

# --skip-api 移除 agent-api
if [[ "$SKIP_API" == true ]]; then
  filtered=()
  for s in "${STEPS[@]}"; do
    [[ "$s" == "agent-api" ]] && continue
    filtered+=("$s")
  done
  STEPS=("${filtered[@]}")
fi

# ─── 执行子脚本 ─────────────────────────────────────────
run_step() {
  local step="$1"
  local script="$DEPLOY_DIR/deploy-${step}.sh"
  [[ -x "$script" ]] || { chmod +x "$script" 2>/dev/null || true; }
  [[ -f "$script" ]] || die "脚本不存在: $script"

  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  log "▶ 步骤: $step"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

  case "$step" in
    showcase)
      if [[ "$SKIP_RENDER" == true ]]; then
        "$script" --skip-render
      else
        "$script"
      fi
      ;;
    *)
      "$script"
      ;;
  esac
}

# ─── 主流程 ─────────────────────────────────────────────
main() {
  log "=== 全量发布 Plotgram ==="
  log "  步骤: ${STEPS[*]}"
  log "  站点: $DEPLOY_HOST ($REMOTE_DIR)"
  log "  CDN:  $ASSET_HOST ($ASSET_REMOTE_DIR)"
  if [[ "$SKIP_RENDER" == true ]]; then
    log "  showcase: 跳过 SVG 渲染"
  fi

  local failed=()
  for step in "${STEPS[@]}"; do
    if ! run_step "$step"; then
      failed+=("$step")
      echo "✗ 步骤失败: $step" >&2
      # 继续执行后续独立步骤，最后汇总
    fi
  done

  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  if [[ ${#failed[@]} -eq 0 ]]; then
    echo "✅ 全量发布完成（${#STEPS[@]} 个步骤全部成功）"
  else
    echo "⚠ 发布完成，但以下步骤失败: ${failed[*]}"
    exit 1
  fi
  echo ""
  echo "   plotgram.cn:"
  echo "     Website:    https://www.plotgram.cn/"
  echo "     Playground: https://www.plotgram.cn/playground/"
  echo "     Showcase:   https://www.plotgram.cn/showcase/"
  echo "     Agent:      https://www.plotgram.cn/agent/"
  echo "   API:          https://api.pg.agcli.cn/health"
  echo "   CDN:          ${CDN_BASE}"
  echo ""
}

main "$@"
