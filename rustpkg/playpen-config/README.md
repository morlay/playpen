# playpen-config

配置聚合层。

## 职责

- `Settings`：`{ default_profile, sandbox, model_providers }`
- `AppConfig`：`{ settings: Settings, sandbox: playpen_sandbox::config::Config }` + 多源合并（settings.toml + conf.d/*.toml + .playpen.toml）
- `Dirs`：`{ working_dir, config_data_dir, agents_dir }`
- `Model` / `ModelProvider` / `ModelProfile` / `ThinkingLevel` / `InputType` / `Cost` 等类型定义
