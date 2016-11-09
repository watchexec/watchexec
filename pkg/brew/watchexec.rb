class Watchexec < Formula
  desc "Execute commands when watched files change"
  homepage "https://github.com/mattgreen/watchexec"
  url "https://github.com/mattgreen/watchexec/releases/download/1.4.0/watchexec-1.4.0-x86_64-apple-darwin.tar.gz"
  sha256 "1ec1ce839be697bef07e1083acdd52ccb108acd29fd638371c1c8ed15e736b6e"

  def install
    bin.install "watchexec"
    man1.install "watchexec.1"
  end

  test do
    system "#{bin}/watchexec", "--version"
  end
end
