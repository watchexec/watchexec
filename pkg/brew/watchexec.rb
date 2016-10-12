class Watchexec < Formula
  desc "Execute commands when watched files change"
  homepage "https://github.com/mattgreen/watchexec"
  url "https://github.com/mattgreen/watchexec/releases/download/0.11.0/watchexec_osx_0.11.0.tar.gz"
  version "0.11.0"
  sha256 "eb11d74ccaff973768e31a7f290c42b77c1f1faeb3c93970cf4f93285af64c39"

  bottle :unneeded

  def install
    bin.install "watchexec"
  end

  test do
    system "#{bin}/watchexec", "--version"
  end
end
