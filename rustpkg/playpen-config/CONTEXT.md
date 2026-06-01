# 配置聚合

TOML 分层合并、路径约定、模型与费用类型。

## 术语

**Dirs**：
三个路径的聚合。`working_dir`（项目根）、`config_data_dir`（`~/.config/playpen`）、`agents_dir`（`~/.agents`）。通过 `with_defaults(cwd)` 构造。
_避免使用_：路径配置、目录服务

**Settings**：
用户可配置的顶层结构。包含 `default_profile`（默认 profile 名称）、`sandbox`（可选沙箱配置）、`model_providers`（provider 映射）。多文件合并：settings.toml → conf.d/*.toml → .playpen.toml，后层覆盖前层。

**AppConfig**：
加载后的完整配置，包含 `Settings` 和已编译的 `playpen_sandbox::config::Config`。`load(cwd)` 执行多源合并，`load_or_default(cwd)` 出错时返回默认值。

**SandboxProfile**：
TOML 中的 `[sandbox]` 段类型。包含 `enabled`、`network`、`filesystem`、`shell` 三个访问维度。由 `playpen-toolkit` 的 sandbox feature 消费，映射为 `playpen_sandbox::config::Config`。
_避免使用_：沙箱策略、安全配置

**ModelProvider**：
LLM 提供商定义。包含 `name`、`base_url`、`api_key`（支持 `${VAR}` 环境变量展开）、`models`（可选模型列表）。预设值由 `preset::providers()` 提供（如 deepseek），用户配置可覆盖。

**Model**：
模型规格。包含 `name`（如 `deepseek-v4-flash`）、`display_name`、`reasoning_efforts`（支持的推理等级）、`input_types`（支持的输入类型）、`context_window`（上下文窗口大小）、`max_tokens`（最大输出 token）、`cost`（每 1M token 单价）。

**ModelProfile**：
Session 维度的模型选择。包含 `model`（如 `deepseek/deepseek-v4-flash`）、`thinking_level`（推理等级）、`temperature`、`top_p`。由 profile 配置的 `default_model_profile` 定义，运行时可通过 ACP `set_config_option` 覆盖。

**Cost**：
每 1M token 的计费单价。`input`、`output`、`cache_read` 分别为输入/输出/缓存命中价格，`currency` 为币种（CNY / USD）。`compute(usage)` 按实际用量计算费用。

**ThinkingLevel**：
推理等级枚举。`Off`（关闭推理）、`High`（高推理）、`Max`（最大推理）。DeepSeek 映射为 `thinking: {type: enabled/disabled}`，OpenAI 映射为 `reasoning_effort` 参数。

**expand_env_vars**：
TOML 值中的 `${VAR}` 或 `$VAR` 环境变量展开。在 `merge_settings` 和 `merge_sandbox` 过程中自动应用。
