#!/usr/bin/env python3
"""Inject prefers-color-scheme dark-mode CSS into criterion's HTML reports.

Criterion bakes its CSS inline into every generated index.html and has no
built-in theming. This script walks target/criterion/**/*.html and adds
two things just before </head>:

  1. <meta name="color-scheme" content="light dark"> — opts the page into
     native dark-mode rendering for scrollbars and form controls.
  2. A separate <style> block with @media (prefers-color-scheme: dark)
     overrides that win cascade-ties because they appear after criterion's
     inline <style>.

A sentinel HTML comment makes the script idempotent. If a previous version
of this script wrote a sentinel inside criterion's <style> tag (which was
invalid HTML and unreliable in some browsers), we detect both and clean it
up before re-injecting.

Usage: python3 scripts/theme-bench-reports.py [target/criterion]
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

SENTINEL = "<!-- cr-theme:applied -->"
LEGACY_SENTINEL = "<!-- cr-theme:applied -->"  # same string, different position

INJECTION = """\
<!-- cr-theme:applied -->
    <meta name="color-scheme" content="light dark">
    <style type="text/css">
        /* cr-theme: prefers-color-scheme adaptive override */
        @media (prefers-color-scheme: dark) {
            html, body { background: #161618; color: #e6e6e6; }
            .body { color: #e6e6e6; }
            a:link, a:visited { color: #6cb6ff; }
            a:hover { color: #9ecbff; }
            h1, h2, h3, h4 { color: #f0f0f0; }
            table, th, td { border-color: #3a3a3d !important; }
            th { background: #232327; color: #f0f0f0; }
            tr:nth-child(even) td { background: #1c1c20; }
            tr:nth-child(odd) td { background: #161618; }
            #footer { background: #232327 !important; color: #d0d0d0; }
            #footer a { color: #f0f0f0; }
            /* Criterion's plots are PNG/SVG with white backgrounds. Invert
               + hue-rotate so chrome adapts; red/blue data colours stay
               approximately correct. */
            img, svg { filter: invert(0.92) hue-rotate(180deg); }
        }
    </style>
"""


def strip_legacy_injection(text: str) -> str:
    """Remove a previous (broken) injection that lived inside criterion's <style>.

    The v1 form put the sentinel + a CSS block before </style> of the original
    style tag. We match from the sentinel up to the first </style> that follows
    and rewrite the file with that span removed. Subsequent fresh injection
    writes the v2 (correct) form.
    """
    pattern = re.compile(
        r"\s*<!-- cr-theme:applied -->\s*"
        r"/\* cr-theme:.*?\}\s*\}\s*",
        re.DOTALL,
    )
    return pattern.sub("\n    ", text)


def theme_file(path: Path) -> str:
    """Inject the dark-mode block into one HTML file. Return one of:
    'themed', 'rethemed', 'skipped' (already correct), or 'noop' (no </head>).
    """
    text = path.read_text(encoding="utf-8")

    # Detect prior v2 injection by looking for the meta tag we add.
    if 'name="color-scheme"' in text and SENTINEL in text:
        return "skipped"

    cleaned = strip_legacy_injection(text)
    legacy_seen = cleaned != text

    # Always anchor on </head> so the injection lives in document head, where
    # both the meta tag and the new <style> tag belong.
    needle = "</head>"
    idx = cleaned.find(needle)
    if idx == -1:
        return "noop"

    new_text = cleaned[:idx] + INJECTION + "    " + cleaned[idx:]
    path.write_text(new_text, encoding="utf-8")
    return "rethemed" if legacy_seen else "themed"


def main(argv: list[str]) -> int:
    root = Path(argv[1] if len(argv) > 1 else "target/criterion")
    if not root.is_dir():
        print(f"error: {root} is not a directory (run `cargo bench` first)", file=sys.stderr)
        return 1

    counts = {"themed": 0, "rethemed": 0, "skipped": 0, "noop": 0}
    for html in root.rglob("*.html"):
        counts[theme_file(html)] += 1

    msg = (
        f"themed {counts['themed']}, "
        f"re-themed {counts['rethemed']} (cleaned legacy injection), "
        f"skipped {counts['skipped']} (already up-to-date), "
        f"noop {counts['noop']} (no </head>)"
    )
    print(msg)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
