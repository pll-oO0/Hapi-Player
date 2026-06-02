# 歌词展示跟唱播放器

一个轻量 Rust 桌面应用，用于导入 LRC 歌词和两轨音频，同步播放歌词、音频和波形。

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

已构建好的程序在：

[`Hapi_Player.exe`](https://github.com/pll-oO0/Hapi-Player/releases/latest)

双击这个文件即可使用，无需安装 Rust，也无需编译。

## 从源码运行

```text
    cargo run
```

首次运行需要下载依赖。若 Cargo 镜像不可用，请检查 `.cargo\config` 或 `config.toml` 中的 registry 配置。

## 说明

本项目覆盖了常见音频格式；极少见或带 DRM 的格式可能无法解码。
