# 列出所有可用入口
[group('meta')]
default:
    @just --list --list-submodules

[group("debug")]
exec cmd *args:
    {{ cmd }} {{ args }}

# Rust 工具链公共入口，用法：just rust test rustpkg/dai-detect
mod rust 'tool/rust'

debug-config:
    cargo run -p playpen -- config

debug-acp:
    cargo run -p playpen -- acp

debug-agent:
    cargo run -p playpen -- agent -i

debug-agent-once:
    cargo run -p playpen -- agent \
        --model=opencode-go/deepseek-v4-flash \
        --thinking-level=off \
        '运行一下 `eza`'
