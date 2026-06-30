class EmbyDl < Formula
  desc "从 Emby 媒体服务器下载视频的命令行工具"
  homepage "https://github.com/vansour/emby-dl"
  license "Unknown"
  version "0.0.5"

  on_macos do
    on_intel do
      url "https://github.com/vansour/emby-dl/releases/download/v#{version}/emby-dl-x86_64-apple-darwin.tar.gz"
      sha256 "79218b9f1377844c3a1659d59eb085d5a1d221e8d54b977e4b9877ae55274c72"
    end
    on_arm do
      url "https://github.com/vansour/emby-dl/releases/download/v#{version}/emby-dl-aarch64-apple-darwin.tar.gz"
      sha256 "9e9663581eb2771720827bd1d909c5b7261ef9fa8e0b6ef75983513ba9b295b3"
    end
  end

  def install
    bin.install "emby-dl"
  end

  test do
    assert_match "emby-dl #{version}", shell_output("#{bin}/emby-dl --version")
  end
end
