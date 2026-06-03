# 歌词展示跟唱播放器

一个轻量 Rust 桌面应用，用于导入 LRC 歌词和两轨音频，同步播放歌词、音频和波形。

支持 **Windows**、**macOS** 和 **Linux**。

## 功能

- 识别 LRC 时间标签，按当前播放时间滚动显示歌词。
- 当前歌词居中高亮，并显示前后各 3 条歌词。
- 支持普通文本歌词文件，没有时间标签时会按行展示。
- 自动识别常见歌词文本编码，包括 UTF-8、UTF-8 BOM、UTF-16LE/BE、GBK/GB18030、Big5、Shift-JIS、EUC-KR、Windows-1252。
- 可导入两轨音频文件。
- 使用 Symphonia 解码常见音频格式，包括 MP3、WAV、FLAC、AAC/M4A、OGG/Vorbis 等。
- 显示两轨音频波形，并用黄色指针标出当前播放位置。
- 播放、暂停、停止和拖动进度条会同步影响歌词、音频和波形指针。

## 直接运行

已构建好的程序可在 [GitHub Releases](https://github.com/pll-oO0/Hapi-Player/releases/latest) 下载：

| 平台 | 下载文件 | 使用方式 |
|------|----------|----------|
| Windows | `hapi-player-windows-x86_64.zip` | 解压后运行 `Hapi_Player.exe` |
| macOS (Apple Silicon) | `hapi-player-macos-aarch64.tar.gz` | 解压后运行 `Hapi_Player.app` |
| macOS (Intel) | `hapi-player-macos-x86_64.tar.gz` | 解压后运行 `Hapi_Player.app` |
| Linux | `hapi-player-linux-x86_64.tar.gz` | 解压后将 `bin/Hapi_Player` 加入 PATH，或直接运行 |

> 首次在 macOS 上运行未签名应用时，可能需要在“系统设置 → 隐私与安全性”中允许打开。

## 从源码运行

```bash
cargo run
```

Release 构建：

```bash
cargo build --release
```

构建产物位于 `target/release/lyrics_follow_player`（Windows 为 `lyrics_follow_player.exe`）。

首次运行需要下载依赖。若 Cargo 镜像不可用，请检查 `.cargo/config.toml` 中的 registry 配置。

### 系统依赖

#### Windows

- 安装 [Rust](https://www.rust-lang.org/tools/install)
- 安装 Visual Studio Build Tools（含 C++ 工具链）
- 中文 UI 会自动尝试加载 `C:\Windows\Fonts\` 下的系统字体

#### macOS

- 安装 [Rust](https://www.rust-lang.org/tools/install)
- 安装 Xcode Command Line Tools：`xcode-select --install`
- 中文 UI 会自动尝试加载 PingFang、Heiti、Songti 等系统字体

#### Linux

除 Rust 外，还需要图形、音频和文件对话框相关系统库。

**Ubuntu / Debian**

```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  pkg-config \
  libgtk-3-dev \
  libxcb-render0-dev \
  libxcb-shape0-dev \
  libxcb-xfixes0-dev \
  libxkbcommon-dev \
  libssl-dev \
  libasound2-dev \
  libudev-dev \
  libwayland-dev \
  libpam0g-dev \
  libdbus-1-dev \
  fonts-noto-cjk
```

**Fedora**

```bash
sudo dnf install -y \
  gcc gcc-c++ pkg-config \
  gtk3-devel \
  libxkbcommon-devel \
  openssl-devel \
  alsa-lib-devel \
  systemd-devel \
  dbus-devel \
  google-noto-sans-cjk-fonts
```

**Arch Linux**

```bash
sudo pacman -S --needed \
  base-devel \
  pkgconf \
  gtk3 \
  libxkbcommon \
  openssl \
  alsa-lib \
  systemd \
  dbus \
  noto-fonts-cjk
```

如果中文显示异常，请安装 Noto CJK 或文泉驿字体包。

## 发布打包

使用 `xtask` 统一构建与打包：

```bash
# 当前平台：构建 + 打包
cargo xtask dist

# 指定 target（CI / 交叉编译）
cargo xtask dist --target x86_64-unknown-linux-gnu

# 仅构建 release，不打包
cargo xtask build
cargo xtask build --target aarch64-apple-darwin
```

产物默认输出到 `dist/`，文件名会根据 target 自动推断，例如 `hapi-player-macos-aarch64.tar.gz`。

打 tag 后会自动触发 GitHub Actions 发布：

```bash
git tag v0.1.0
git push origin v0.1.0
```

各平台产物说明：

- **Windows**：`Hapi_Player.exe`，打包为 zip
- **macOS**：`Hapi_Player.app`，打包为 tar.gz
- **Linux**：`bin/Hapi_Player` 与 `.desktop` 文件，打包为 tar.gz

## 说明

本项目覆盖了常见音频格式；极少见或带 DRM 的格式可能无法解码。

macOS 面向普通用户分发时，后续可补充代码签名与 notarization；Linux 后续可按需要增加 AppImage、`.deb` 或 `.rpm` 打包。
