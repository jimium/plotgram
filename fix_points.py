#!/usr/bin/env python3
import re
import os
import subprocess

ROOT = "/Users/jimichan/zaprt-projects/flowml"
CORE = os.path.join(ROOT, "crates/drawify-core")

def get_error_files():
    result = subprocess.run(
        ["cargo", "check", "-p", "drawify-core"],
        cwd=ROOT,
        capture_output=True,
        text=True
    )
    files = set()
    for line in result.stderr.split("\n"):
        m = re.search(r"-->\s*(crates/drawify-core/src/[^\s:]+)", line)
        if m:
            files.add(os.path.join(ROOT, m.group(1)))
    return list(files)

def add_point_import(content):
    if "use crate::layout::geometry::Point;" in content:
        return content
    
    uses_layout_types = any(kw in content for kw in [
        "PathGeometry", "EdgeLayout", "EdgeLabelLayout", 
        "path_start", "path_end", "path_points", "label_pos",
        "node_center", "point_at_path_t", "closest_point_on_path",
        "polyline_points", "bezier_controls", "sampled_path"
    ])
    
    if not uses_layout_types:
        return content
    
    lines = content.split("\n")
    
    for i, line in enumerate(lines):
        if "use crate::layout::geometry" in line:
            if "Point" not in line:
                lines[i] = line.rstrip().rstrip(";").rstrip(",") + ", Point;"
                content = "\n".join(lines)
            return content
    
    insert_idx = None
    for i, line in enumerate(lines):
        if line.strip().startswith("use crate::layout::") and "geometry" not in line:
            insert_idx = i
            break
    
    if insert_idx is not None:
        indent = len(lines[insert_idx]) - len(lines[insert_idx].lstrip())
        indent_str = " " * indent
        lines.insert(insert_idx, f"{indent_str}use crate::layout::geometry::Point;")
        content = "\n".join(lines)
    
    return content

def fix_point_field_access(content):
    replacements = [
        (r'(\.path_start\(\)(?:\.[a-zA-Z_()]+)*)\.0\b', r'\1.x'),
        (r'(\.path_start\(\)(?:\.[a-zA-Z_()]+)*)\.1\b', r'\1.y'),
        (r'(\.path_end\(\)(?:\.[a-zA-Z_()]+)*)\.0\b', r'\1.x'),
        (r'(\.path_end\(\)(?:\.[a-zA-Z_()]+)*)\.1\b', r'\1.y'),
        (r'(\.label_pos\(\)(?:\.[a-zA-Z_()]+)*)\.0\b', r'\1.x'),
        (r'(\.label_pos\(\)(?:\.[a-zA-Z_()]+)*)\.1\b', r'\1.y'),
        (r'(\.label_pos_at\([^)]+\)(?:\.[a-zA-Z_()]+)*)\.0\b', r'\1.x'),
        (r'(\.label_pos_at\([^)]+\)(?:\.[a-zA-Z_()]+)*)\.1\b', r'\1.y'),
        (r'(node_center\([^)]+\)(?:\.[a-zA-Z_()]+)*)\.0\b', r'\1.x'),
        (r'(node_center\([^)]+\)(?:\.[a-zA-Z_()]+)*)\.1\b', r'\1.y'),
        (r'(\.center)\.0\b', r'\1.x'),
        (r'(\.center)\.1\b', r'\1.y'),
    ]
    
    for pattern, replacement in replacements:
        content = re.sub(pattern, replacement, content)
    
    return content

def process_file(filepath):
    print(f"Processing: {filepath}")
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    original = content
    content = add_point_import(content)
    content = fix_point_field_access(content)
    
    if content != original:
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(content)
        print(f"  Modified!")
    else:
        print(f"  No changes.")

if __name__ == "__main__":
    os.chdir(ROOT)
    files = get_error_files()
    print(f"Found {len(files)} files with errors:")
    for f in files:
        print(f"  - {os.path.basename(f)}")
    
    for filepath in files:
        if os.path.exists(filepath) and "graphic_style" not in filepath:
            process_file(filepath)
