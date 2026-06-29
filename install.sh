#!/usr/bin/env bash
set -euo pipefail

REPO="vansour/emby-dl"
INSTALL_DIR="/opt/emby-dl"
BIN_NAME="emby-dl"

arch_to_target() {
    local arch
    arch=$(uname -m)
    case "$arch" in
        x86_64)  echo "x86_64-unknown-linux-gnu" ;;
        aarch64) echo "aarch64-unknown-linux-gnu" ;;
        i686|i386) echo "i686-unknown-linux-gnu" ;;
        armv7l)  echo "armv7-unknown-linux-gnueabihf" ;;
        *)
            echo "不支持的架构: $arch" >&2
            exit 1
            ;;
    esac
}

echo "获取最新版本..."
latest=$(curl -sSfL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": "\(.*\)",.*/\1/')

if [ -z "$latest" ]; then
    echo "错误: 无法获取最新版本" >&2
    exit 1
fi
echo "最新版本: $latest"

target=$(arch_to_target)
archive="$BIN_NAME-$target.tar.gz"
download_url="https://github.com/$REPO/releases/download/$latest/$archive"

echo "下载: $download_url"
curl -sSfL -o "/tmp/$archive" "$download_url"

echo "安装到 $INSTALL_DIR"
sudo mkdir -p "$INSTALL_DIR"
sudo tar xzf "/tmp/$archive" -C "$INSTALL_DIR"
sudo chmod +x "$INSTALL_DIR/$BIN_NAME"
rm -f "/tmp/$archive"

if [ ! -e /usr/local/bin/$BIN_NAME ]; then
    echo "创建符号链接: /usr/local/bin/$BIN_NAME"
    sudo ln -sf "$INSTALL_DIR/$BIN_NAME" /usr/local/bin/$BIN_NAME
fi

echo "安装完成! 运行 emby-dl --help 开始使用"
