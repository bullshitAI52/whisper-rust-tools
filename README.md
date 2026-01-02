# Whisper & Media Tools (Rust 重构版)

本项目是原有 Python 媒体处理工具的 Rust 重构版本，旨在提供更高性能的音频转写和媒体处理能力。

## 项目包含应用

### 1. `whisper_app`
基于 OpenAI Whisper 模型和 Rust `candle` 框架的 GUI 应用，用于视频/音频的字幕生成。
- **核心引擎**: [Candle](https://github.com/huggingface/candle) (Rust 机器学习框架)
- **主要功能**: 
  - 音频提取
  - 语音活动检测 (VAD)
  - 字幕生成 (SRT 格式)
  - 支持本地 GPU/CPU 推理加速

### 2. `media_cutter`
用于精确视频和音频剪辑的 GUI 应用。
- **主要功能**:
  - 快速定位和预览
  - 无损剪辑 (在条件允许时)
  - 简洁的时间轴操作界面

## 编译指南

编译前请确保已安装 [Rust](https://rustup.rs/) 环境。

```bash
# 编译两个应用的 Release 版本
cargo build --release
```

编译完成后，二进制文件将位于 `target/release/` 目录下。

## 下载

您可以从 [Releases](https://github.com/bullshitAI52/whisper-rust-tools/releases) 页面下载最新构建的 Windows (`.exe`) 和 macOS 版本。
