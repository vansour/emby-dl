class EmbyDl < Formula
  desc "从 Emby 媒体服务器下载视频的命令行工具"
  homepage "https://github.com/vansour/emby-dl"
  license "Unknown"
  version "0.0.9"

  on_macos do
    on_intel do
      url "https://github.com/vansour/emby-dl/releases/download/v#{version}/emby-dl-x86_64-apple-darwin.tar.gz"
      sha256 "78ba9b7f5654d98d0181ff609f8b96325033ef6b14ad0f1cbf21cd1e9dbea1dc"
    end
    on_arm do
      url "https://github.com/vansour/emby-dl/releases/download/v#{version}/emby-dl-aarch64-apple-darwin.tar.gz"
      sha256 "c12b4266277dafc43ec0dc0a4a721c6437f36a93329d628fa9c31f0b8b6d65b1"
    end
  end

  def install
    bin.install "emby-dl"
  end

  test do
    assert_match "emby-dl #{version}", shell_output("#{bin}/emby-dl --version")
  end
end
