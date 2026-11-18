#!/usr/bin/env python3
"""
Deterministic Python 3 standard-library-only script to render terminal capture .txt files
into crisp dark-terminal SVG images matching Beacon and Streamline styling rules.
Background: #0b1429, Border: #3f577c, rx: 10, monospace #edf5ff text with #94ead1 highlight
on 'ok' / 'test result: ok' lines. Deterministic output bytes for identical inputs.
"""

import os
import sys
import glob
import html
import tempfile

def render_svg(txt_content: str) -> str:
    lines = txt_content.splitlines()
    if not lines:
        lines = [""]
    
    # Compute dimensions
    char_width = 8.5
    char_height = 20
    padding_x = 24
    padding_y = 28
    header_height = 36
    
    max_len = max(len(line) for line in lines) if lines else 0
    width = int(max(720, max_len * char_width + (padding_x * 2)))
    height = int(header_height + (len(lines) * char_height) + (padding_y * 2))
    
    svg_lines = []
    svg_lines.append(f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">')
    svg_lines.append('  <defs>')
    svg_lines.append('    <style>')
    svg_lines.append('      .term-bg { fill: #0b1429; stroke: #3f577c; stroke-width: 1.5; rx: 10; }')
    svg_lines.append('      .header-bg { fill: #101a35; rx: 10; }')
    svg_lines.append('      .dot-red { fill: #ff5f56; }')
    svg_lines.append('      .dot-yellow { fill: #ffbd2e; }')
    svg_lines.append('      .dot-green { fill: #27c93f; }')
    svg_lines.append('      .title { fill: #8ab4f8; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 13px; font-weight: 600; }')
    svg_lines.append('      .code { fill: #edf5ff; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 13px; }')
    svg_lines.append('      .highlight { fill: #94ead1; font-weight: 600; }')
    svg_lines.append('    </style>')
    svg_lines.append('  </defs>')
    
    # Background and border box
    svg_lines.append(f'  <rect x="1" y="1" width="{width-2}" height="{height-2}" class="term-bg" />')
    
    # Window header bar
    svg_lines.append(f'  <path d="M 1,10 q 0,-9 9,-9 h {width-20} q 9,0 9,9 v {header_height-1} h -{width-2} v -{header_height-1}" fill="#101a35" />')
    svg_lines.append('  <circle cx="24" cy="18" r="6" class="dot-red" />')
    svg_lines.append('  <circle cx="44" cy="18" r="6" class="dot-yellow" />')
    svg_lines.append('  <circle cx="64" cy="18" r="6" class="dot-green" />')
    svg_lines.append(f'  <text x="{width / 2}" y="22" text-anchor="middle" class="title">streamline-terminal</text>')
    
    # Separator line under header
    svg_lines.append(f'  <line x1="1" y1="{header_height}" x2="{width-1}" y2="{header_height}" stroke="#3f577c" stroke-width="1" />')
    
    # Render text lines
    start_y = header_height + padding_y + 4
    for i, line in enumerate(lines):
        y = start_y + (i * char_height)
        stripped = line.strip().lower()
        is_hl = "ok" in stripped or "test result: ok" in stripped or "success" in stripped or "passed" in stripped
        cls = "code highlight" if is_hl else "code"
        escaped = html.escape(line)
        svg_lines.append(f'  <text x="{padding_x}" y="{y}" class="{cls}">{escaped}</text>')
        
    svg_lines.append('</svg>')
    return "\n".join(svg_lines) + "\n"

def main():
    captures_dir = os.path.join(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
        "captures",
    )
    
    if os.path.islink(captures_dir):
        print(f"Error: captures directory {captures_dir} is a symbolic link.", file=sys.stderr)
        sys.exit(1)
        
    if not os.path.exists(captures_dir):
        print(f"Captures directory {captures_dir} does not exist.")
        sys.exit(0)
        
    txt_files = glob.glob(os.path.join(captures_dir, "*.txt"))
    if not txt_files:
        print(f"No .txt capture files found in {captures_dir}.")
        sys.exit(0)
        
    for txt_path in sorted(txt_files):
        if os.path.islink(txt_path):
            print(f"Error: capture file {txt_path} is a symbolic link.", file=sys.stderr)
            sys.exit(1)
            
        svg_path = os.path.splitext(txt_path)[0] + ".svg"
        if os.path.islink(svg_path):
            print(f"Error: output path {svg_path} is a symbolic link.", file=sys.stderr)
            sys.exit(1)
        with open(txt_path, "r", encoding="utf-8") as f:
            content = f.read()
        svg_content = render_svg(content)
        
        tmp_name = None
        try:
            with tempfile.NamedTemporaryFile(
                mode="w",
                encoding="utf-8",
                dir=captures_dir,
                prefix=".capture-",
                suffix=".svg.tmp",
                delete=False,
            ) as tmp:
                tmp_name = tmp.name
                tmp.write(svg_content)
                tmp.flush()
                os.fsync(tmp.fileno())
            os.chmod(tmp_name, 0o644)
            os.replace(tmp_name, svg_path)
        except Exception:
            if tmp_name and os.path.exists(tmp_name):
                try:
                    os.unlink(tmp_name)
                except Exception:
                    pass
            raise
            
        print(f"Rendered {os.path.basename(svg_path)}")

if __name__ == "__main__":
    main()
