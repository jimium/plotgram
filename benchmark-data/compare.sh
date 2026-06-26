#!/bin/bash
# 对比分析优化前后数据
# 用法: bash compare.sh

cd /workspace/benchmark-data

echo "## 全量架构图性能对比"
echo ""
echo "| 文件 | 边数 | 基线(ms) | 优化(ms) | 加速 | 友好性变化 | 交叉变化 | 退化变化 |"
echo "|------|------|----------|----------|------|-----------|----------|----------|"

# 使用 Python 来做对比（更可靠）
python3 << 'PYEOF'
import csv

def read_csv(path):
    rows = {}
    with open(path) as f:
        reader = csv.DictReader(f)
        for row in reader:
            rows[row['file']] = row
    return rows

baseline = read_csv('baseline.csv')
optimized = read_csv('optimized.csv')

total_base = 0
total_opt = 0
friendly_changes = []
cross_changes = []
degraded_changes = []

for fname in sorted(baseline.keys()):
    b = baseline[fname]
    o = optimized.get(fname)
    if not o:
        continue
    
    edges = b['edges']
    b_ms = float(b['median_ms'])
    o_ms = float(o['median_ms'])
    speedup = b_ms / o_ms if o_ms > 0 else 0
    total_base += b_ms
    total_opt += o_ms
    
    # 友好性
    b_f = b['friendly']
    o_f = o['friendly']
    if b_f and o_f:
        b_fv = float(b_f)
        o_fv = float(o_f)
        f_diff = o_fv - b_fv
        f_str = f"{f_diff:+.2f}" if abs(f_diff) > 0.005 else "不变"
        friendly_changes.append(f_diff)
    else:
        f_str = "N/A"
    
    # 预测交叉
    b_c = int(b['pred_cross'])
    o_c = int(o['pred_cross'])
    c_diff = o_c - b_c
    c_str = f"{c_diff:+d}" if c_diff != 0 else "不变"
    cross_changes.append(c_diff)
    
    # 退化
    b_d = int(b['degraded'])
    o_d = int(o['degraded'])
    d_diff = o_d - b_d
    d_str = f"{d_diff:+d}" if d_diff != 0 else "不变"
    degraded_changes.append(d_diff)
    
    # 判断加速率
    if speedup > 2.5:
        speed_str = f"**{speedup:.1f}x**"
    elif speedup > 1.5:
        speed_str = f"{speedup:.1f}x"
    else:
        speed_str = f"{speedup:.1f}x"
    
    print(f"| {fname} | {edges} | {b_ms:.1f} | {o_ms:.1f} | {speed_str} | {f_str} | {c_str} | {d_str} |")

print(f"\n| **合计** | - | **{total_base:.1f}** | **{total_opt:.1f}** | **{total_base/total_opt:.1f}x** | - | - | - |")

# 分析退化情况
print("\n## 退化分析\n")
print(f"- 总耗时: {total_base:.0f}ms → {total_opt:.0f}ms, 整体加速 **{total_base/total_opt:.1f}x**")

# 友好性退化
f_degraded = [x for x in friendly_changes if x < -0.01]
f_improved = [x for x in friendly_changes if x > 0.01]
f_same = [x for x in friendly_changes if abs(x) <= 0.01]
print(f"- 友好性: {len(f_improved)} 改善, {len(f_same)} 不变, {len(f_degraded)} 退化")

# 交叉退化
c_degraded = [x for x in cross_changes if x > 0]
c_improved = [x for x in cross_changes if x < 0]
c_same = [x for x in cross_changes if x == 0]
print(f"- 预测交叉: {len(c_improved)} 减少, {len(c_same)} 不变, {len(c_degraded)} 增加")

# 退化数
d_degraded = [x for x in degraded_changes if x > 0]
d_improved = [x for x in degraded_changes if x < 0]
d_same = [x for x in degraded_changes if x == 0]
print(f"- 退化路由: {len(d_improved)} 减少, {len(d_same)} 不变, {len(d_degraded)} 增加")

# 超过100ms的文件
print("\n## 超过 100ms 的文件\n")
for fname in sorted(baseline.keys()):
    b = baseline[fname]
    o = optimized.get(fname)
    if not o:
        continue
    b_ms = float(b['median_ms'])
    o_ms = float(o['median_ms'])
    if b_ms > 100:
        print(f"- **{fname}**: {b_ms:.0f}ms → {o_ms:.0f}ms ({b_ms/o_ms:.1f}x)")
    elif o_ms > 100:
        print(f"- **{fname}** (优化后仍超100ms): {b_ms:.0f}ms → {o_ms:.0f}ms")

PYEOF