class EmbyDl < Formula
  desc "从 Emby 媒体服务器下载视频的命令行工具"
  homepage "https://github.com/vansour/emby-dl"
  license "Unknown"
  version "0.0.9"

  on_macos do
    on_intel do
      url "https://github.com/vansour/emby-dl/releases/download/v#{version}/emby-dl-x86_64-apple-darwin.tar.gz"
      sha256 "fa62068bebb346af69ae6f4c9592f75f3a382d862f595392891b8280615a7327"
    end
    on_arm do
      url "https://github.com/vansour/emby-dl/releases/download/v#{version}/emby-dl-aarch64-apple-darwin.tar.gz"
      sha256 "930eacddc8a3a00b74d1abbc81db12667dcf5ac8265233b243183fd429e80984"
    end
  end

  def install
    bin.install "emby-dl"
  end

  test do
    assert_match "emby-dl #{version}", shell_output("#{bin}/emby-dl --version")
  end
end
