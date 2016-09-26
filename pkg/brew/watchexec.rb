class Watchexec < Formula
  desc "Execute commands when watched files change"
  homepage "https://github.com/mattgreen/watchexec"
  url "https://github.com/mattgreen/watchexec/releases/download/0.10.0/watchexec_osx_0.10.0.tar.gz"
  version "0.10.0"
  sha256 "0a2eae6fc0f88614bdc732b233b0084a3aa6208177c52c4d2ecb0a671f55d82b"

  bottle :unneeded

  def install
    bin.install "watchexec"
  end

  test do
    system "#{bin}/watchexec", "--version"
  end
end
