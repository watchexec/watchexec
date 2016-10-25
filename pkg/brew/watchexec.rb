class Watchexec < Formula
  desc "Execute commands when watched files change"
  homepage "https://github.com/mattgreen/watchexec"
  url "https://github.com/mattgreen/watchexec/releases/download/1.2.0/watchexec-1.2.0-x86_64-apple-darwin.tar.gz"
  sha256 "38784ca4442630eb094f53a50ca95c87d870798842c2a11525a816b1d8bf383c"

  def install
    bin.install "watchexec"
  end

  test do
    system "#{bin}/watchexec", "--version"
  end
end
