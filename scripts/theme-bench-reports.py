#!/usr/bin/env python3
"""Inject prefers-color-scheme dark-mode CSS into criterion's HTML reports.

Criterion bakes its CSS inline into every generated index.html and has no
built-in theming. This script walks target/criterion/**/*.html and appends
a `@media (prefers-color-scheme: dark)` override block to each page's
existing <style> tag. Runs are idempotent: a sentinel comment is added
on first pass, and subsequent passes skip already-themed files.

Light-mode users see no change; dark-mode users see dark.

Usage: python3 scripts/theme-bench-reports.py [target/criterion]
"""

from __future__ import annotations

import sys
from pathlib import Path

SENTINEL = "<!-- cr-theme:applied -->"

# CSS lives inside the existing <style> tag, so it shares specificity with
# the original rules. Order matters: our rules come *after* criterion's, so
# they win cascade ties without needing !important.
DARK_CSS = """
        /* cr-theme: prefers-color-scheme adaptive override */
        @media (prefers-color-scheme: dark) {
            body { background: #161618; color: #e6e6e6; }
            .body { color: #e6e6e6; }
            a:link, a:visited { color: #6cb6ff; }
            a:hover { color: #9ecbff; }
            h2, h3, h4 { color: #f0f0f0; }
            table, th, td { border-color: #3a3a3d !important; }
            th { background: #232327; color: #f0f0f0; }
            tr:nth-child(even) td { background: #1c1c20; }
            tr:nth-child(odd) td { background: #161618; }
            #footer { background: #232327 !important; color: #d0d0d0; }
            #footer a { color: #f0f0f0; }
            /* Criterion's plots are PNG/SVG images with white backgrounds.
               Invert + hue-rotate so the chrome adapts; data colours stay
               approximately correct (red stays warm, blue stays cool). */
            img { filter: invert(0.92) hue-rotate(180deg); }
        }
"""


def theme_file(path: Path) -> bool:
    """Inject the dark-mode block into one HTML file. Return True if changed."""
    text = path.read_text(encoding="utf-8")
    if SENTINEL in text:
        return False
    needle = "</style>"
    idx = text.find(needle)
    if idx == -1:
        return False
    injected = SENTINEL + "\n" + DARK_CSS + "    "
    new_text = text[:idx] + injected + text[idx:]
    path.write_text(new_text, encoding="utf-8")
    return True


def main(argv: list[str]) -> int:
    root = Path(argv[1] if len(argv) > 1 else "target/criterion")
    if not root.is_dir():
        print(f"error: {root} is not a directory (run `cargo bench` first)", file=sys.stderr)
        return 1

    changed = 0
    skipped = 0
    for html in root.rglob("*.html"):
        if theme_file(html):
            changed += 1
        else:
            skipped += 1

    print(f"themed {changed} file(s); skipped {skipped} (already themed or no <style>)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
