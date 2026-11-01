class Tiles < Formula
  desc "Charts and dashboards in your terminal, drawn by your AI coding tool"
  homepage "https://github.com/tileshq/tiles"
  # Formulae cannot put `url` inside `on_arm` / `on_intel`, so pick the
  # release tarball for this Mac's architecture here.
  if Hardware::CPU.arm?
    url "https://github.com/tileshq/tiles/releases/download/v0.2.0/tiles-v0.2.0-aarch64-apple-darwin.tar.gz"
    sha256 "f00f2127b8eb0520b9351fe5655cd1bdefd795bbcf5ed5840fea543d9f440d06"
  else
    url "https://github.com/tileshq/tiles/releases/download/v0.2.0/tiles-v0.2.0-x86_64-apple-darwin.tar.gz"
    sha256 "49f96aff9e57e91559887a45c5212bd3c9219980bd471639865ade9f22a76b4b"
  end
  license "FSL-1.1-ALv2"

  depends_on :macos

  def install
    bin.install "tiles"
  end

  def caveats
    <<~EOS
      Next, run `tiles setup`. It finds the coding tools on your PATH
      and asks once before it changes anything.
    EOS
  end

  test do
    assert_match "0.2.0", shell_output("#{bin}/tiles --version")
  end
end
