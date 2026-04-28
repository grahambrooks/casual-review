class CasualReview < Formula
  desc "Ultra-fast code review CLI with rustc-quality diagnostics"
  homepage "https://github.com/grahambrooks/casual-review"
  version "2026.4.28"
  license "MIT OR Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/grahambrooks/casual-review/releases/download/v2026.4.28/cr-v2026.4.28-aarch64-apple-darwin.tar.gz"
      sha256 "06a9ebff7acd62ed96ad08ce6a34619112765bd63997e62cee44f4a15c2862aa"
    end
    on_intel do
      odie "Intel Mac binaries are not provided. Run `cargo install --git https://github.com/grahambrooks/casual-review --locked` to build from source."
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/grahambrooks/casual-review/releases/download/v2026.4.28/cr-v2026.4.28-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "ce7f748a300648f449046d79273028f36f3b37f2c04918b19af5e44fdc44b794"
    end
    on_intel do
      url "https://github.com/grahambrooks/casual-review/releases/download/v2026.4.28/cr-v2026.4.28-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "93617993bc534b42fc241bef583e32d197a8894eb7a24b7185bf4ed3d5ec6fdf"
    end
  end

  def install
    bin.install "cr"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/cr --version")
  end
end
