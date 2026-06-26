#!/usr/bin/env python3
"""
智能修复脚本：基于 cargo check 的错误提示，逐个修复编译错误。
只处理编译器明确指出的问题。
"""
import re
import os
import subprocess
import sys

ROOT = "/Users/jimichan/zaprt-projects/flowml"

def run_cargo_check():
    result = subprocess.run(
        ["cargo", "check", "-p", "drawify-core"],
        cwd=ROOT,
        capture_output=True,
        text=True
    )
    return result.stdout + result.stderr

def parse_errors(output):
    """解析 cargo check 错误，返回错误列表"""
    errors = []
    
    # 模式: 错误类型，文件，行号
    # error[E0609]: no field `0` on type `...`
    #   --> crates/drawify-core/src/...:line:col
    
    lines = output.split('\n')
    i = 0
    current_error = None
    
    while i < len(lines):
        line = lines[i]
        
        # 新错误开始
        em = re.match(r'^error(?:\[E\d+\])?:\s*(.*)', line)
        if em:
            current_error = {'message': em.group(1), ' fixes': []}
            errors.append(current_error)
            i += 1
            continue
        
        # 文件位置
        lm = re.match(r'^\s*-->\s*(crates/drawify-core/src/[^:]+):(\d+):(\d+)', line)
        if lm and current_error is not None:
            current_error['file'] = os.path.join(ROOT, lm.group(1))
            current_error['line'] = int(lm.group(2))
            current_error['col'] = int(lm.group(3))
            i += 1
            # 读取接下来的几行找 help 建议
            help_text = []
            for j in range(1, 15):
                if i + j >= len(lines):
                    break
                hline = lines[i + j]
                help_text.append(hline)
                if hline.strip().startswith('help:') or hline.strip().startswith('= help:'):
                    # 继续看替换建议
                    for k in range(j+1, min(j+5, len(lines)-i)):
                        if i + k >= len(lines):
                            break
                        help_text.append(lines[i + k])
                    break
                if hline.strip() == '' or hline.startswith('error') or hline.startswith('warning'):
                    break
            current_error['help'] = '\n'.join(help_text)
            i += 1
            continue
        
        i += 1
    
    return errors

def apply_field_access_fix(filepath, line_num, col_num, old_field, new_field):
    """修复字段访问错误：把 .0 改成 .x 或 .1 改成 .y"""
    with open(filepath, 'r', encoding='utf-8') as f:
        file_lines = f.readlines()
    
    if line_num < 1 or line_num > len(file_lines):
        return False
    
    idx = line_num - 1
    line = file_lines[idx]
    
    # col_num 是 1-based，字段访问是 .0 或 .1
    # 在列位置查找 .0 或 .1
    # 注意：列指向的是字段名（0或1），不是点号
    col_idx = col_num - 1
    
    # 从 col_idx 位置往回找 '.'，然后替换
    if col_idx < len(line):
        # 检查 col_idx 是否是 0 或 1
        if col_idx < len(line) and line[col_idx] in ('0', '1'):
            # 找前面的 '.'
            dot_idx = col_idx - 1
            while dot_idx >= 0 and line[dot_idx].isspace():
                dot_idx -= 1
            if dot_idx >= 0 and line[dot_idx] == '.':
                # 替换这个位置的字段
                new_line = line[:col_idx] + new_field + line[col_idx+1:]
                file_lines[idx] = new_line
                with open(filepath, 'w', encoding='utf-8') as f:
                    f.writelines(file_lines)
                print(f"  Fixed: {os.path.basename(filepath)}:{line_num} .{old_field} -> .{new_field}")
                return True
    
    # 备用方法：在整行中找第一个出现的 .0/.1（在上下文正确的情况下）
    # 不太安全，先不用
    return False

def add_point_import(filepath):
    """添加 Point 导入"""
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    if "use crate::layout::geometry::Point;" in content:
        return False
    
    # 检查是否需要导入
    if not any(kw in content for kw in ["Point", "PathGeometry", "EdgeLayout", "path_start", "path_end"]):
        return False
    
    lines = content.split('\n')
    
    # 找是否有 geometry 模块的导入
    for i, line in enumerate(lines):
        if "use crate::layout::geometry" in line:
            if "Point" not in line:
                # 修改导入
                lines[i] = line.rstrip().rstrip(';').rstrip(',') + ", Point;"
                with open(filepath, 'w', encoding='utf-8') as f:
                    f.write('\n'.join(lines))
                print(f"  Added Point to existing geometry import in {os.path.basename(filepath)}")
                return True
            return False
    
    # 找第一个 use crate::layout 行
    for i, line in enumerate(lines):
        if line.strip().startswith("use crate::layout::") and "geometry" not in line and "{" not in line:
            indent = len(line) - len(line.lstrip())
            indent_str = " " * indent
            lines.insert(i, f"{indent_str}use crate::layout::geometry::Point;")
            with open(filepath, 'w', encoding='utf-8') as f:
                f.write('\n'.join(lines))
            print(f"  Added Point import to {os.path.basename(filepath)}")
            return True
    
    return False

def main():
    max_iterations = 20
    
    for iteration in range(max_iterations):
        print(f"\n=== Iteration {iteration + 1} ===")
        output = run_cargo_check()
        
        errors = parse_errors(output)
        
        # 过滤出有文件位置的错误
        field_errors = [e for e in errors if 'file' in e and 
                       ('no field `0`' in e['message'] or 'no field `1`' in e['message'] or
                        'no field `0`' in e.get('help', '') or 'no field `1`' in e.get('help', ''))]
        
        # 统计错误总数
        error_count = len([e for e in errors if 'file' in e])
        print(f"Found {error_count} errors")
        
        if error_count == 0:
            print("No more errors! Success!")
            return True
        
        fixes_applied = 0
        
        # 处理字段访问错误
        for err in field_errors:
            if 'no field `0`' in err['message']:
                if apply_field_access_fix(err['file'], err['line'], err['col'], '0', 'x'):
                    fixes_applied += 1
            elif 'no field `1`' in err['message']:
                if apply_field_access_fix(err['file'], err['line'], err['col'], '1', 'y'):
                    fixes_applied += 1
        
        # 如果没有字段错误可以修复，尝试添加 Point 导入
        if fixes_applied == 0:
            # 尝试为有错误的文件添加 Point 导入
            files_with_errors = set(e.get('file') for e in errors if 'file' in e)
            for f in files_with_errors:
                if f and add_point_import(f):
                    fixes_applied += 1
                    break  # 一次只添加一个，重新检查
        
        if fixes_applied == 0:
            print("Could not fix any errors automatically. Remaining errors:")
            for e in errors[:10]:
                if 'file' in e:
                    print(f"  {os.path.basename(e['file'])}:{e['line']}: {e['message'][:80]}")
            return False
    
    print(f"Reached max iterations ({max_iterations})")
    return False

if __name__ == "__main__":
    os.chdir(ROOT)
    success = main()
    sys.exit(0 if success else 1)
