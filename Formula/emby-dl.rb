class EmbyDl < Formula
  desc "从 Emby 媒体服务器下载视频的命令行工具"
  homepage "https://github.com/vansour/emby-dl"
  license "Unknown"
  version "0.0.4"

  on_macos do
    on_intel do
      url "https://github.com/vansour/emby-dl/releases/download/v#{version}/emby-dl-x86_64-apple-darwin.tar.gz"
      sha256 "5c30a73cd3ce3a7f9a93a580c7b7df10a3dc09bc77f52e0ec1d54fe9068b6192"
    end
    on_arm do
      url "https://github.com/vansour/emby-dl/releases/download/v#{version}/emby-dl-aarch64-apple-darwin.tar.gz"
      sha256 "f3b6b2d1ae27e7a17a04fa7d75ab687e950de16d75eb7e1ecb430ab1db9d5549"
    end
  end

  def install
    bin.install "emby-dl"
  end

  test do
    assert_match "emby-dl #{version}", shell_output("#{bin}/emby-dl --version")
  end
end
