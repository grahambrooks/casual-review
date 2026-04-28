class CasualReview < Formula
  desc "Ultra-fast code review CLI with rustc-quality diagnostics"
  homepage "https://github.com/grahambrooks/casual-review"
  version "2026.4.0"
  license "MIT OR Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/grahambrooks/casual-review/releases/download/v2026.4.0/cr-v2026.4.0-aarch64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    on_intel do
      url "https://github.com/grahambrooks/casual-review/releases/download/v2026.4.0/cr-v2026.4.0-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/grahambrooks/casual-review/releases/download/v2026.4.0/cr-v2026.4.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    on_intel do
      url "https://github.com/grahambrooks/casual-review/releases/download/v2026.4.0/cr-v2026.4.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  def install
    bin.install "cr"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/cr --version")
  end
end
