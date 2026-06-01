# 列出所有可用入口
[group('meta')]
default:
    @just --list --list-submodules

[group("debug")]
exec cmd *args:
    {{ cmd }} {{ args }}

# Rust 工具链公共入口，用法：just rust test rustpkg/dai-detect
mod rust 'tool/rust'