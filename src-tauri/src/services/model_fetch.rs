//! Model list fetch service
//!
//! Fetches a provider's available model list via the OpenAI-compatible
//! GET /v1/models endpoint. Mainly targets third-party aggregators (SiliconFlow,
//! OpenRouter, etc.), as well as official providers that mount the Anthropic
//! protocol on a compatible sub-path (DeepSeek, Kimi, Zhipu GLM, etc.).

#[cfg(test)]
mod tests {}
