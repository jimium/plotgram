#!/usr/bin/env python3
"""Convert .dfy entity declarations from old { type: x } syntax to new entity[x] syntax."""

import os
import re
import sys


def is_sequence_diagram(lines):
    for line in lines:
        stripped = line.strip()
        if stripped.startswith('diagram '):
            return 'sequence' in stripped
    return False


def parse_string(s, start):
    if start >= len(s) or s[start] != '"':
        return None, None
    i = start + 1
    while i < len(s):
        if s[i] == '\\':
            i += 2
            continue
        if s[i] == '"':
            return i + 1, s[start:i+1]
        i += 1
    return None, None


def find_matching_brace(lines, start_line, brace_col):
    depth = 0
    in_string = False
    string_quote = None
    i = start_line
    col = brace_col
    while i < len(lines):
        line = lines[i]
        start_col = col if i == start_line else 0
        j = start_col
        while j < len(line):
            c = line[j]
            if in_string:
                if c == '\\':
                    j += 2
                    continue
                if c == string_quote:
                    in_string = False
                j += 1
            else:
                if c in ('"', "'"):
                    in_string = True
                    string_quote = c
                    j += 1
                elif c == '{':
                    depth += 1
                    j += 1
                elif c == '}':
                    depth -= 1
                    if depth == 0:
                        return i, j
                    j += 1
                else:
                    j += 1
        i += 1
        col = 0
    return -1, -1


def convert_file(filepath):
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()

    lines = content.split('\n')
    is_sequence = is_sequence_diagram(lines)

    entity_start = re.compile(r'^(\s*)entity\s+(\S+)\s+"')

    new_lines = []
    i = 0
    modified = False

    while i < len(lines):
        line = lines[i]
        match = entity_start.match(line)

        if not match:
            new_lines.append(line)
            i += 1
            continue

        indent = match.group(1)
        entity_id = match.group(2)

        label_start = match.end() - 1
        label_end, label_str = parse_string(line, label_start)

        if label_end is None:
            new_lines.append(line)
            i += 1
            continue

        after_label = line[label_end:]

        brace_pos = after_label.find('{')
        if brace_pos == -1:
            if after_label.strip() == '':
                new_lines.append(line)
                i += 1
                continue
            else:
                new_lines.append(line)
                i += 1
                continue

        brace_col = label_end + brace_pos
        close_line, close_col = find_matching_brace(lines, i, brace_col)

        if close_line == -1:
            new_lines.append(line)
            i += 1
            continue

        # Extract block content
        block_lines = []
        if close_line == i:
            block_text = line[brace_col+1:close_col].strip()
            if block_text:
                block_lines.append(block_text)
        else:
            first_content = line[brace_col+1:].strip()
            if first_content:
                block_lines.append(first_content)
            for bi in range(i+1, close_line):
                block_lines.append(lines[bi])
            last_content = lines[close_line][:close_col].strip()
            if last_content:
                block_lines.append(last_content)

        # Parse attributes
        type_val = None
        other_attrs = []
        attr_pattern = re.compile(r'^(\s*)(\S+):\s*(.+?)\s*$')

        for bl in block_lines:
            bl_stripped = bl.strip()
            if not bl_stripped:
                other_attrs.append(bl)
                continue
            am = attr_pattern.match(bl)
            if am and am.group(2) == 'type':
                type_val = am.group(3).strip()
            else:
                other_attrs.append(bl)

        if is_sequence and type_val == 'entity':
            type_val = 'lifeline'

        if type_val is None:
            new_lines.append(line)
            i += 1
            continue

        while other_attrs and other_attrs[0].strip() == '':
            other_attrs.pop(0)
        while other_attrs and other_attrs[-1].strip() == '':
            other_attrs.pop()

        if not other_attrs:
            new_line = f'{indent}entity[{type_val}] {entity_id} {label_str}'
            new_lines.append(new_line)
            modified = True
        else:
            new_line = f'{indent}entity[{type_val}] {entity_id} {label_str} {{'
            new_lines.append(new_line)
            for oa in other_attrs:
                new_lines.append(oa)
            new_lines.append(f'{indent}}}')
            modified = True

        i = close_line + 1

    if modified:
        new_content = '\n'.join(new_lines)
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(new_content)

    return modified


def main():
    root_dir = '/workspace/showcase'
    count = 0
    modified_files = []

    for dirpath, dirnames, filenames in os.walk(root_dir):
        dirnames[:] = [d for d in dirnames if not d.startswith('.')]

        for fname in filenames:
            if fname.endswith('.dfy'):
                fpath = os.path.join(dirpath, fname)
                if convert_file(fpath):
                    count += 1
                    modified_files.append(fpath)

    print(f"Modified {count} files:")
    for f in sorted(modified_files):
        rel = os.path.relpath(f, root_dir)
        print(f"  {rel}")


if __name__ == '__main__':
    main()
