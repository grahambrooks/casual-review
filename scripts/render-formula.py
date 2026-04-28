#!/usr/bin/env python3
"""Render the Homebrew formula with a given version and platform SHA256s.

Usage: render-formula.py VERSION SHA_DARWIN_ARM SHA_DARWIN_INTEL SHA_LINUX_ARM SHA_LINUX_INTEL
Output is the formula text on stdout.
"""

from __future__ import annotations

import sys

TEMPLATE = """\
class CasualReview < Formula
  desc "Ultra-fast code review CLI with rustc-quality diagnostics"
  homepage "https://github.com/grahambrooks/casual-review"
  version "{version}"
  license "MIT OR Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/grahambrooks/casual-review/releases/download/v{version}/cr-v{version}-aarch64-apple-darwin.tar.gz"
      sha256 "{sha_darwin_arm}"
    end
    on_intel do
      url "https://github.com/grahambrooks/casual-review/releases/download/v{version}/cr-v{version}-x86_64-apple-darwin.tar.gz"
      sha256 "{sha_darwin_intel}"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/grahambrooks/casual-review/releases/download/v{version}/cr-v{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "{sha_linux_arm}"
    end
    on_intel do
      url "https://github.com/grahambrooks/casual-review/releases/download/v{version}/cr-v{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "{sha_linux_intel}"
    end
  end

  def install
    bin.install "cr"
  end

  test do
    assert_match version.to_s, shell_output("#{{bin}}/cr --version")
  end
end
"""


def main(argv: list[str]) -> int:
    if len(argv) != 6:
        print(__doc__, file=sys.stderr)
        return 2

    print(
        TEMPLATE.format(
            version=argv[1],
            sha_darwin_arm=argv[2],
            sha_darwin_intel=argv[3],
            sha_linux_arm=argv[4],
            sha_linux_intel=argv[5],
        ),
        end="",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
