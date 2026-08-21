# 图片经 DeepSeek Files API 上传后以 file_id 引用

## 背景

ACP 客户端传入的图片类内容（`ContentBlock::Resource::Blob` / `ResourceLink`）此前被
`format_content_block` 文本化（base64 文本）发给 LLM：模型看到的是 base64 字符串而非图片，
且 rig-core 0.41 的 DeepSeek provider 会把 content 数组平铺为字符串、静默丢弃非文本块。

[DeepSeek Files API](https://api-docs.deepseek.com/guides/files_api) 要求图片先经
`POST /files`（multipart，`purpose=user_data`）上传获得 `file_id`，再以
`{"type":"file","file_id":"..."}` 内容块引用（配合 `deepseek-v4-flash-vision-exp`）。

## 决策

1. **升级 rig-core 0.41 → 0.42**：0.42 的 DeepSeek provider 对含非文本块的 content 数组
   不再扁平化（`flatten_text_content_parts(..., only_if_all_text=true)`），多模态块得以
   透传；同时 `StreamFinal` 标准化了 finish_reason / usage，移除 downcast 提取逻辑。
2. **自定义 provider `PlaypenDeepSeekExt`**（playpen-agent/client.rs）：在 rig 内置
   DeepSeek 行为基础上，把 OpenAI 格式的 file 块 `{"type":"file","file":{"file_id":...}}`
   改写为 DeepSeek 格式 `{"type":"file","file_id":...}`。rig 未提供该改写 hook 的公开入口
   （`flatten_text_content_parts` 为 `pub(crate)`），故复制其语义到本地。
3. **图片上传器 `DeepSeekImageUploader`**（playpen-agent/files.rs）：上传图片 → `file_id`；
   以内容 sha256 去重（进程内缓存 + session `StateUpdate` 持久化，resume 复用）。
4. **转换层注入**：`events_to_chat_history` / `event_to_user_message` 接受可选
   `ImageUploader`；仅当 provider 为 `deepseek` 且模型名为 vision（`is_vision_model`）
   时由 runner 注入。图片块上传后转为 `UserContent::Document(FileId)`（wire 层改写为
   DeepSeek file 块）；未注入 uploader 或上传失败时回退文本化。
5. **持久化语义不变**：session 中 UserMessage 保留原始图片块（blob），上传只发生在
   「发送给 LLM 之前」的转换阶段，ACP replay 仍能看到原始图片。

## 副作用修复

- `ContentBlock::Resource` 的 serde 内部 tag 与 `ContentBlock` 外层 tag 同名（`type`），
  嵌套序列化产生重复键，DB 读回时 `content` 反序列化失败（`unwrap_or_default()` 回退为空）。
  已将内部 tag 改为 `resource_type`（playpen-content/content.rs），并补充 roundtrip 回归测试。

## 限制

- 仅图片（JPEG/PNG/GIF/WebP，media_type 或扩展名判定）走 Files API；
  非图片资源（PDF 等）维持文本化。
- 文件永久保留（未传 `expires_after`）；跨 session 不共享 file_id（缓存键含 session state）。
