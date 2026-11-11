//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::common::data::{Message, RemoteError, Usage};
use crate::json_parser::{JsonField, autoparser};
use crate::{Context as _, OrtResult};

// {
//  "id":"gen-1756743299-7ytIBcjALWQQShwMQfw9",
//  "provider":"Meta",
//  "model":"meta-llama/llama-3.3-8b-instruct:free",
//  "object":"chat.completion.chunk",
//  "created":1756743300,
//  "choices":[
//      {
//      "index":0,
//      "delta":{"role":"assistant","content":""},
//      "finish_reason":null,
//      "native_finish_reason":null,
//      "logprobs":null
//      }
//  ],
//  "usage":{
//      "prompt_tokens":42,
//      "completion_tokens":2,
//      "total_tokens":44,
//      "cost":0,"
//      is_byok":false,
//      "prompt_tokens_details":{"cached_tokens":0,"audio_tokens":0},
//      "cost_details":{"upstream_inference_cost":null,"upstream_inference_prompt_cost":0,"upstream_inference_completions_cost":0},
//      "completion_tokens_details":{"reasoning_tokens":0,"image_tokens":0}}
//  }

pub struct ChatCompletionsResponse {
    // Data parsed from remote
    pub provider: Option<String>,
    pub model: Option<String>,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
    pub error: Option<RemoteError>,
}

impl ChatCompletionsResponse {
    pub fn from_json(json: &str) -> OrtResult<Self> {
        let mut fields = [
            JsonField::new_simple_string("provider"),
            JsonField::new_simple_string("model"),
            JsonField::new_vec_raw("choices"),
            JsonField::new_raw("usage"),
            JsonField::new_raw("error"),
        ];
        autoparser(json, &mut fields).context("ChatCompletionsResponse autoparser")?;

        let model_id = fields[1].get_string();

        let mut choices = vec![];
        if let Some(v) = fields[2].get_vec_raw() {
            for c in v {
                choices.push(Choice::from_json(&c).context("Choice")?);
            }
        }

        let usage = fields[3]
            .get_raw()
            .as_deref()
            .map(Usage::from_json)
            .transpose()?;

        let error = fields[4]
            .get_raw()
            .as_deref()
            .map(RemoteError::from_json)
            .transpose()?;

        Ok(ChatCompletionsResponse {
            provider: fields[0].get_string(),
            model: model_id.clone(),
            choices,
            usage,
            error,
        })
    }
}

pub struct Choice {
    pub delta: Message,
    pub finish_reason: Option<String>,
}

impl Choice {
    pub fn is_tool_call_finish(&self) -> bool {
        matches!(self.finish_reason.as_deref(), Some("tool_calls"))
    }

    pub fn from_json(json: &str) -> OrtResult<Self> {
        let mut fields = [
            JsonField::new_raw("delta"),
            JsonField::new_simple_string("finish_reason"),
        ];
        autoparser(json, &mut fields)?;
        let delta_json = fields[0].get_raw().expect("Missing delta in message");

        Ok(Choice {
            delta: Message::from_json(&delta_json).context("Message")?,
            finish_reason: fields[1].get_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_choice() {
        let s = r#"{"index":0,"delta":{"role":"assistant","content":"Hello"},"finish_reason":"stop","native_finish_reason":"stop","logprobs":null}"#;
        let choice = Choice::from_json(s).unwrap();
        assert_eq!(choice.delta.text(), Some("Hello"));
    }

    #[test]
    fn test_chat_completions_response_simple() {
        let arr = [
            r#"{"id":"gen-1756743299-7ytIBcjALWQQShwMQfw9","provider":"Meta","model":"meta-llama/llama-3.3-8b-instruct:free","object":"chat.completion.chunk","created":1756743300,"choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756743299-7ytIBcjALWQQShwMQfw9","provider":"Meta","model":"meta-llama/llama-3.3-8b-instruct:free","object":"chat.completion.chunk","created":1756743300,"choices":[{"index":0,"delta":{"role":"assistant","content":"Hello"},"finish_reason":"stop","native_finish_reason":"stop","logprobs":null}]}"#,
            r#"{"id":"gen-1756743299-7ytIBcjALWQQShwMQfw9","provider":"Meta","model":"meta-llama/llama-3.3-8b-instruct:free","object":"chat.completion.chunk","created":1756743300,"choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null,"native_finish_reason":null,"logprobs":null}],"usage":{"prompt_tokens":42,"completion_tokens":2,"total_tokens":44,"cost":0,"is_byok":false,"prompt_tokens_details":{"cached_tokens":0,"audio_tokens":0},"cost_details":{"upstream_inference_cost":null,"upstream_inference_prompt_cost":0,"upstream_inference_completions_cost":0},"completion_tokens_details":{"reasoning_tokens":0,"image_tokens":0}}}"#,
        ];
        for a in arr {
            let ccr = ChatCompletionsResponse::from_json(a).unwrap();
            assert_eq!(ccr.provider.as_deref(), Some("Meta"));
            assert_eq!(
                ccr.model.as_deref(),
                Some("meta-llama/llama-3.3-8b-instruct:free")
            );
            assert_eq!(ccr.choices.len(), 1);
        }
    }

    #[test]
    fn test_chat_completions_response_more() {
        let arr = [
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"Rea","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"l","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":" Madrid, 14 times.","reasoning":null,"reasoning_details":[]},"finish_reason":"stop","native_finish_reason":"stop","logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null,"native_finish_reason":null,"logprobs":null}],"usage":{"prompt_tokens":33,"completion_tokens":8,"total_tokens":41,"cost":0.0000310365,"is_byok":false,"prompt_tokens_details":{"cached_tokens":0,"audio_tokens":0},"cost_details":{"upstream_inference_cost":null,"upstream_inference_prompt_cost":0.00001815,"upstream_inference_completions_cost":0.0000132},"completion_tokens_details":{"reasoning_tokens":0,"image_tokens":0}}}"#,
        ];
        for a in arr {
            let ccr = ChatCompletionsResponse::from_json(a).unwrap();
            assert_eq!(ccr.provider.as_deref(), Some("WandB"));
            assert_eq!(ccr.model.as_deref(), Some("deepseek/deepseek-chat-v3.1"));
            assert_eq!(ccr.choices.len(), 1);
        }
    }

    // Various null fields, including inside the message, and usage.
    #[test]
    fn test_nvidia_misc() {
        let s = r#"{"id":"8f20d6699e194a0abed38c671384d32d","object":"chat.completion.chunk","created":1770582573,"model":"qwen/qwen3-next-80b-a3b-instruct","choices":[{"index":0,"delta":{"role":null,"content":"Ta","reasoning_content":null,"tool_calls":null},"logprobs":null,"finish_reason":null,"matched_stop":null}],"usage":null}"#;
        let ccr = ChatCompletionsResponse::from_json(s).unwrap();
        assert_eq!(ccr.choices[0].delta.text(), Some("Ta"));
    }

    #[test]
    fn test_parse_chat_error() {
        let json = r#"{"id":"gen-1787255401-CBB2yYetBs5rQ1TxQ3zc","object":"chat.completion.chunk","created":1787255401,"model":"openai/gpt-5.6-terra","provider":"OpenAI","choices":[],"error":{"code":400,"message":"Server tool request failed","metadata":{"error_type":"invalid_request"}}}"#;
        let ccr = ChatCompletionsResponse::from_json(json).unwrap();
        assert_eq!(ccr.error.unwrap().message(), "Server tool request failed");
    }

    #[test]
    fn test_preserves_reasoning_details() {
        let json = r#"{"id":"gen-1788467537-QS8BVdZN4vsO5mtNhoN1","object":"chat.completion.chunk","created":1788467537,"model":"openai/gpt-5.6-sol","provider":"OpenAI","choices":[{"index":0,"delta":{"content":"","role":"assistant","reasoning":null,"reasoning_details":[{"type":"reasoning.encrypted","data":"gAAAAABqJ9","format":"openai-responses-v1","id":"rs_05a6389df207c51e016a99d959ecd887d1b672d700b322a752","index":0}]},"finish_reason":null,"native_finish_reason":null}]}"#;
        let ccr = ChatCompletionsResponse::from_json(json).unwrap();
        let Some(ref rd) = ccr.choices.first().unwrap().delta.reasoning_details else {
            panic!("Missing reasoning_details");
        };
        assert!(rd.contains("reasoning.encrypted"));
    }
}
