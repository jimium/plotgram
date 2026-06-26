#!/bin/bash
# 全量架构图基准测试脚本
# 用法: ./bench-all.sh [runs]

set -euo pipefail

RUNS="${1:-5}"
BENCH_DIR="/workspace/benchmark-data"
BINARY="/workspace/target/release/bench-phases"
ARCH_DIR="/workspace/showcase/architecture"

cd /workspace

# 确保 binary 是最新的
cargo build --release -p drawify-core --bin bench-phases 2>/dev/null

echo "file,nodes,edges,groups,parse_ms,median_ms,min_ms,max_ms,friendly,congestion,long_edge,gap,pred_cross,port_conflict,candidates,hard_reject,degraded"

for f in "$ARCH_DIR"/*.dfy; do
    fname=$(basename "$f")
    output=$("$BINARY" "$f" "$RUNS" 2>/dev/null)
    
    # 提取节点/边/分组数
    nodes=$(echo "$output" | grep "节点:" | head -1 | sed 's/.*节点: \([0-9]*\).*/\1/')
    edges=$(echo "$output" | grep "节点:" | head -1 | sed 's/.*边: \([0-9]*\).*/\1/')
    groups=$(echo "$output" | grep "节点:" | head -1 | sed 's/.*分组: \([0-9]*\).*/\1/')
    
    # 解析耗时
    parse_ms=$(echo "$output" | grep "解析+prepare 耗时:" | sed 's/.*耗时:[[:space:]]*\([0-9.]*\)ms.*/\1/')
    
    # 路由耗时
    median_ms=$(echo "$output" | grep "中位数:" | sed 's/.*中位数:[[:space:]]*\([0-9.]*\)ms.*/\1/')
    min_ms=$(echo "$output" | grep "最小值:" | sed 's/.*最小值:[[:space:]]*\([0-9.]*\)ms.*/\1/')
    max_ms=$(echo "$output" | grep "最大值:" | sed 's/.*最大值:[[:space:]]*\([0-9.]*\)ms.*/\1/')
    
    # 友好性
    friendly=$(echo "$output" | grep "路由友好性:" | sed 's/.*路由友好性: \([0-9.]*\).*/\1/')
    congestion=$(echo "$output" | grep "拥堵分数:" | sed 's/.*拥堵分数:[[:space:]]*\([0-9]*\).*/\1/')
    long_edge=$(echo "$output" | grep "长边分数:" | sed 's/.*长边分数:[[:space:]]*\([0-9]*\).*/\1/')
    gap=$(echo "$output" | grep "间隙充足度:" | sed 's/.*间隙充足度:[[:space:]]*\([0-9]*\).*/\1/')
    pred_cross=$(echo "$output" | grep "预测交叉:" | sed 's/.*预测交叉:[[:space:]]*\([0-9]*\).*/\1/')
    port_conflict=$(echo "$output" | grep "端口冲突:" | sed 's/.*端口冲突:[[:space:]]*\([0-9]*\).*/\1/')
    
    # 正交统计
    candidates=$(echo "$output" | grep "候选总数:" | sed 's/.*候选总数:[[:space:]]*\([0-9]*\).*/\1/')
    hard_reject=$(echo "$output" | grep "硬过滤拒绝:" | sed 's/.*硬过滤拒绝:[[:space:]]*\([0-9]*\).*/\1/')
    degraded=$(echo "$output" | grep "退化数:" | sed 's/.*退化数:[[:space:]]*\([0-9]*\).*/\1/')
    
    echo "$fname,$nodes,$edges,$groups,$parse_ms,$median_ms,$min_ms,$max_ms,$friendly,$congestion,$long_edge,$gap,$pred_cross,$port_conflict,$candidates,$hard_reject,$degraded"
done