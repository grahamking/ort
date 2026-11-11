//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::ThinkEvent;
use crate::{
    Context as _, ErrorKind, OrtResult, StreamEvent,
    common::{data::ToolCall, stats},
    json_parser::{JsonField, autoparser},
    ort_err, time,
};

pub fn from_json(data: &str) -> OrtResult<ResponseEvent> {
    ResponseEvent::from_json(data)
}

pub fn extract_events(
    mut v: ResponseEvent,
    _tsc_calibration: Option<time::TscCalibration>,
    queue: &mut Vec<StreamEvent>,
    _stats: &mut stats::Stats,
    _pending_tool_calls: &mut Vec<ToolCall>,
) {
    use ResponseEvent::*;
    match v {
        Created => queue.push(StreamEvent::Start),
        ReasoningTextDelta(s) => queue.push(StreamEvent::Think(ThinkEvent::Content(s))),
        OutputTextDelta(s) => queue.push(StreamEvent::Content(s)),
        Completed => queue.push(StreamEvent::Finish("complete".to_string())),
        _ => {}
    }
}

#[derive(Debug)]
pub enum ResponseEvent {
    Created,
    InProgress,
    OutputItemAdded,
    ContentPartAdded,
    ReasoningTextDelta(String),
    ReasoningTextDone,
    ContentPartDone,
    OutputItemDone,
    OutputTextDelta(String),
    OutputTextDone,
    Completed, // TODO: This needs a Stats
}

impl ResponseEvent {
    pub fn from_json(json: &str) -> OrtResult<Self> {
        let mut fields = [JsonField::new_simple_string("type")];
        autoparser(json, &mut fields).context("ResponseEvent autoparser")?;

        let event_type = fields[0].get_string().unwrap();
        //crate::print_string(c"Type: ", &event_type);
        match event_type.as_str() {
            "response.created" => Ok(ResponseEvent::Created),
            "response.in_progress" => Ok(ResponseEvent::InProgress),
            "response.output_item.added" => Ok(ResponseEvent::OutputItemAdded),
            "response.content_part.added" => Ok(ResponseEvent::ContentPartAdded),
            "response.reasoning_text.delta" => {
                let mut fields = [JsonField::new_string("delta")];
                autoparser(json, &mut fields)
                    .context("ResponseEvent reasoning_text.delta autoparser")?;
                Ok(ResponseEvent::ReasoningTextDelta(
                    fields[0].get_string().unwrap(),
                ))
            }
            "response.reasoning_text.done" => Ok(ResponseEvent::ReasoningTextDone),
            "response.content_part.done" => Ok(ResponseEvent::ContentPartDone),
            "response.output_item.done" => Ok(ResponseEvent::OutputItemDone),
            "response.output_text.delta" => {
                let mut fields = [JsonField::new_string("delta")];
                autoparser(json, &mut fields)
                    .context("ResponseEvent output_text.delta autoparser")?;
                Ok(ResponseEvent::OutputTextDelta(
                    fields[0].get_string().unwrap(),
                ))
            }
            "response.output_text.done" => Ok(ResponseEvent::OutputTextDone),
            "response.completed" => Ok(ResponseEvent::Completed),
            _ => Err(ort_err(ErrorKind::FormatError, "TODO".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use core::assert_matches;

    use super::*;

    #[test]
    fn test_created() {
        let s = r#"{"type":"response.created","response":{"id":"gen-1793455435-3289GeKfwfWDlN5b2cKR","object":"response","created_at":1793455435,"model":"openai/gpt-oss-20b","status":"in_progress","completed_at":null,"output":[],"error":null,"incomplete_details":null,"access_programs":null,"tools":[],"tool_choice":"auto","parallel_tool_calls":true,"max_output_tokens":null,"temperature":1,"top_p":1,"presence_penalty":0,"frequency_penalty":0,"top_logprobs":0,"max_tool_calls":null,"metadata":{},"background":false,"previous_response_id":null,"service_tier":"auto","truncation":"disabled","store":false,"instructions":null,"text":{"format":{"type":"text"}},"reasoning":null,"safety_identifier":null,"prompt_cache_key":null,"prompt_cache_options":null,"usage":null},"sequence_number":0}"#;
        let r = ResponseEvent::from_json(s).unwrap();
        assert_matches!(r, ResponseEvent::Created);
    }

    #[test]
    fn test_in_progress() {
        let s = r#"{"type":"response.in_progress","response":{"id":"gen-1793455435-3289GeKfwfWDlN5b2cKR","object":"response","created_at":1793455435,"model":"openai/gpt-oss-20b","status":"in_progress","completed_at":null,"output":[],"error":null,"incomplete_details":null,"access_programs":null,"tools":[],"tool_choice":"auto","parallel_tool_calls":true,"max_output_tokens":null,"temperature":1,"top_p":1,"presence_penalty":0,"frequency_penalty":0,"top_logprobs":0,"max_tool_calls":null,"metadata":{},"background":false,"previous_response_id":null,"service_tier":"auto","truncation":"disabled","store":false,"instructions":null,"text":{"format":{"type":"text"}},"reasoning":null,"safety_identifier":null,"prompt_cache_key":null,"prompt_cache_options":null,"usage":null},"sequence_number":1}"#;
        let r = ResponseEvent::from_json(s).unwrap();
        assert_matches!(r, ResponseEvent::InProgress);
    }

    #[test]
    fn test_output_item_added() {
        let s = r#"{"type":"response.output_item.added","output_index":0,"item":{"id":"rs_tmp_b18h8au8kt","type":"reasoning","status":"in_progress","summary":[]},"sequence_number":2}"#;
        let r = ResponseEvent::from_json(s).unwrap();
        assert_matches!(r, ResponseEvent::OutputItemAdded);
    }

    #[test]
    fn test_content_part_added() {
        let s = r#"{"type":"response.content_part.added","output_index":0,"item_id":"rs_tmp_b18h8au8kt","content_index":0,"part":{"type":"reasoning_text","text":""},"sequence_number":3}"#;
        let r = ResponseEvent::from_json(s).unwrap();
        assert_matches!(r, ResponseEvent::ContentPartAdded);
    }

    #[test]
    fn test_reasoning_text_delta() {
        let s = r#"{"type":"response.reasoning_text.delta","output_index":0,"item_id":"rs_tmp_b18h8au8kt","content_index":0,"delta":"The","sequence_number":4}"#;
        let r = ResponseEvent::from_json(s).unwrap();
        let ResponseEvent::ReasoningTextDelta(delta) = r else {
            panic!("Expected ReasoningDelta");
        };
        assert_eq!(delta, "The");
    }

    #[test]
    fn test_reasoning_text_done() {
        let s = r#"{"type":"response.reasoning_text.done","output_index":0,"item_id":"rs_tmp_b18h8au8kt","content_index":0,"text":"The user asks: \"What is the capital of Cape Verde?\" The answer: Praia. Provide concise answer.","sequence_number":26}"#;
        let r = ResponseEvent::from_json(s).unwrap();
        assert_matches!(r, ResponseEvent::ReasoningTextDone);
    }

    #[test]
    fn test_content_part_done() {
        let s = r#"{"type":"response.content_part.done","output_index":0,"item_id":"rs_tmp_b18h8au8kt","content_index":0,"part":{"type":"reasoning_text","text":"The user asks: \"What is the capital of Cape Verde?\" The answer: Praia. Provide concise answer."},"sequence_number":27}"#;
        let r = ResponseEvent::from_json(s).unwrap();
        assert_matches!(r, ResponseEvent::ContentPartDone);
    }

    #[test]
    fn test_output_item_done() {
        let s = r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"rs_tmp_b18h8au8kt","type":"reasoning","status":"completed","content":[{"type":"reasoning_text","text":"The user asks: \"What is the capital of Cape Verde?\" The answer: Praia. Provide concise answer."}],"summary":[],"format":"unknown"},"sequence_number":28}"#;
        let r = ResponseEvent::from_json(s).unwrap();
        assert_matches!(r, ResponseEvent::OutputItemDone);
    }

    #[test]
    fn test_output_text_delta() {
        let s = r#"{"type":"response.output_text.delta","output_index":1,"item_id":"msg_tmp_utgyk7j722m","content_index":0,"delta":" capital","logprobs":[],"sequence_number":32}"#;
        let r = ResponseEvent::from_json(s).unwrap();
        let ResponseEvent::OutputTextDelta(delta) = r else {
            panic!("Expected ReasoningDelta");
        };
        assert_eq!(delta, " capital");
    }

    #[test]
    fn test_output_text_done() {
        let s = r#"{"type":"response.output_text.done","output_index":1,"item_id":"msg_tmp_utgyk7j722m","content_index":0,"text":"The capital of Cape Verde is **Praia**.","logprobs":[],"sequence_number":42}"#;
        let r = ResponseEvent::from_json(s).unwrap();
        assert_matches!(r, ResponseEvent::OutputTextDone);
    }

    #[test]
    fn test_response_completed() {
        let s = r#"{"type":"response.completed","response":{"id":"gen-1793455435-3289GeKfwfWDlN5b2cKR","object":"response","created_at":1793455435,"model":"openai/gpt-oss-20b","status":"completed","completed_at":1793455435,"output":[{"id":"rs_tmp_b18h8au8kt","type":"reasoning","status":"completed","content":[{"type":"reasoning_text","text":"The user asks: \"What is the capital of Cape Verde?\" The answer: Praia. Provide concise answer."}],"summary":[],"format":"unknown"},{"id":"msg_tmp_utgyk7j722m","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"The capital of Cape Verde is **Praia**.","annotations":[],"logprobs":[]}]}],"error":null,"incomplete_details":null,"access_programs":null,"tools":[],"tool_choice":"auto","parallel_tool_calls":true,"max_output_tokens":null,"temperature":1,"top_p":1,"presence_penalty":0,"frequency_penalty":0,"top_logprobs":0,"max_tool_calls":null,"metadata":{},"background":false,"previous_response_id":null,"service_tier":"auto","truncation":"disabled","store":false,"instructions":null,"text":{"format":{"type":"text"}},"reasoning":{"effort":"medium"},"safety_identifier":null,"prompt_cache_key":null,"prompt_cache_options":null,"usage":{"input_tokens":75,"input_tokens_details":{"cached_tokens":0},"output_tokens":43,"output_tokens_details":{"reasoning_tokens":22},"total_tokens":118,"cost":0.00000522,"is_byok":false,"cost_details":{"upstream_inference_cost":0.00000522,"upstream_inference_input_cost":0.00000135,"upstream_inference_output_cost":0.00000387}}},"sequence_number":45}"#;
        let r = ResponseEvent::from_json(s).unwrap();
        assert_matches!(r, ResponseEvent::Completed);
    }
}
