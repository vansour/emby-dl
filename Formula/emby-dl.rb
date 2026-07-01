class EmbyDl < Formula
  desc "从 Emby 媒体服务器下载视频的命令行工具"
  homepage "https://github.com/vansour/emby-dl"
  license "Unknown"
  version "0.0.8"

  on_macos do
    on_intel do
      url "https://github.com/vansour/emby-dl/releases/download/v#{version}/emby-dl-x86_64-apple-darwin.tar.gz"
      sha256 "89cf53748943c4b9daf9f61adfcdcb3ac1e2a06415087c3c2edf9ab68e95b8dc"
    end
    on_arm do
      url "https://github.com/vansour/emby-dl/releases/download/v#{version}/emby-dl-aarch64-apple-darwin.tar.gz"
      sha256 "e4036b02e823e7941a59af5f30278338f0b8899264fa0eec66ffbcaf2045f78d"
    end
  end

  def install
    bin.install "emby-dl"
  end

  test do
    assert_match "emby-dl #{version}", shell_output("#{bin}/emby-dl --version")
  end
end
