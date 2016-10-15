class Watchexec < Formula
  desc "Execute commands when watched files change"
  homepage "https://github.com/mattgreen/watchexec"
  url "https://github.com/mattgreen/watchexec/releases/download/1.0.0/watchexec-1.0.0-x86_64-apple-darwin.tar.gz"
  version "1.0.0"
  sha256 "151d8f8075dfb88a69b1d5d6f9b43c849aa1e099a3357fab494b1f43092ed59c"

  def install
    bin.install "watchexec"
  end

  test do
    system "#{bin}/watchexec", "--version"
  end
end
