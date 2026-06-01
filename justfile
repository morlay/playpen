# 列出所有可用入口
[group('meta')]
default:
    @just --list --list-submodules

[group("debug")]
exec cmd *args:
    {{ cmd }} {{ args }}

# Rust 工具链公共入口，用法：just rust test rustpkg/dai-detect
mod rust 'tool/rust'

playpen := 'cargo run -p playpen --release --'

playpen-config:
    {{ playpen }} config

playpen-acp:
    {{ playpen }} acp

playpen-agent:
    {{ playpen }} agent -i

prompt-code-once prompt="读一下 mise.toml 的内容，输出出来就好":
    {{ playpen }} agent \
        --model=deepseek/deepseek-v4-flash \
        --thinking-level=off \
        --profile=code \
        '{{ prompt }}'

prompt-once prompt="读一下 mise.toml 的内容，输出出来就好":
    PLAYPEN_LOG_DIR={{ justfile_directory() }}/target/log-debug \
        {{ playpen }} agent \
        --model=deepseek/deepseek-v4-flash \
        --thinking-level=high \
        '{{ prompt }}'

session-list *args:
    {{ playpen }} agent session list {{ args }}

session id=`just session-list --limit=1 | jq '.id'`:
    {{ playpen }} agent session get {{ id }} 

install-profile-code:
    ln -sfn {{ justfile_directory() }}/example/profiles/code ~/.config/playpen/profiles/code
