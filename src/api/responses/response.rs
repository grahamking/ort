//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::common::data::Usage;
use crate::{Annotation, Function, ThinkEvent};
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
    v: ResponseEvent,
    tsc_calibration: Option<time::TscCalibration>,
    queue: &mut Vec<StreamEvent>,
    stats: &mut stats::Stats,
    pending_tool_calls: &mut Vec<ToolCall>,
) {
    use ResponseEvent::*;

    match v {
        Created => queue.push(StreamEvent::Start),
        Queued => queue.push(StreamEvent::Warn("Request queued".to_string())),
        ReasoningTextDelta(s) => {
            if let Some(tsc) = tsc_calibration {
                stats.record_first_token(tsc);
            }
            stats.num_tokens += 1;
            queue.push(StreamEvent::Think(ThinkEvent::Content(s)));
        }
        OutputTextDelta(s) => {
            if let Some(tsc) = tsc_calibration {
                stats.record_first_token(tsc);
            }
            stats.num_tokens += 1;
            queue.push(StreamEvent::Content(s));
        }
        Annotation(a) => queue.push(StreamEvent::Annotation(a)),
        FunctionCallStart(tool_call) => {
            pending_tool_calls.push(tool_call);
        }
        FunctionCallArgumentsDone(tool_call) => {
            match pending_tool_calls
                .iter_mut()
                .find(|item| item.index == tool_call.index)
            {
                Some(pending) => {
                    pending.update_from(&tool_call);
                }
                None => {
                    unreachable!("FunctionCallArgumentsDone without FunctionCallStart");
                }
            }
        }
        Completed(st) => {
            *stats += st;
            if pending_tool_calls.is_empty() {
                // End of user turn
                if let Some(tc) = tsc_calibration {
                    stats.record_final_timing(tc);
                }
                queue.push(StreamEvent::Stats(stats.clone()));
                queue.push(StreamEvent::Finish("complete".to_string()));
            } else {
                // End of tool turn, run the tool calls
                let tcs = StreamEvent::ToolCalls(core::mem::take(pending_tool_calls));
                queue.push(tcs);
            }
        }
        Failed { code, message } => {
            queue.push(StreamEvent::Error(code + " " + &message));
        }
        _ => {}
    }
}

#[derive(Debug)]
pub enum ResponseEvent {
    Created,
    Queued,
    InProgress,
    OutputItemAdded,
    ContentPartAdded,
    ReasoningTextDelta(String),
    ReasoningTextDone,
    ReasoningSummaryAdded,
    ReasoningSummaryDone,
    ContentPartDone,
    OutputItemDone,
    OutputTextDelta(String),
    OutputTextDone,
    Annotation(Annotation),
    FunctionCallStart(ToolCall),
    FunctionCallArgumentDelta,
    FunctionCallArgumentsDone(ToolCall),
    Completed(stats::Stats),
    Failed { code: String, message: String },
}

impl ResponseEvent {
    pub fn from_json(json: &str) -> OrtResult<Self> {
        let mut fields = [JsonField::new_simple_string("type")];
        autoparser(json, &mut fields).context("ResponseEvent autoparser")?;

        let event_type = fields[0].get_string().unwrap();
        //crate::print_string(c"Type: ", &event_type);
        match event_type.as_str() {
            "response.created" => Ok(ResponseEvent::Created),
            "response.queued" => Ok(ResponseEvent::Queued),
            "response.in_progress" => Ok(ResponseEvent::InProgress),
            "response.output_item.added" => {
                let mut fields = [
                    JsonField::new_raw("item"),
                    JsonField::new_int("output_index"),
                ];
                autoparser(json, &mut fields)
                    .context("ResponseEvent output_item.added autoparser")?;

                let item_json = fields[0].get_raw().unwrap();
                let item_index = fields[1].get_int().unwrap();
                let mut fields = [
                    JsonField::new_simple_string("type"),
                    // These three for type: "function_call"
                    JsonField::new_simple_string("call_id"),
                    JsonField::new_simple_string("name"),
                    JsonField::new_string("arguments"),
                ];
                autoparser(&item_json, &mut fields)
                    .context("ResponseEvent output_item.added autoparser")?;
                if fields[0].get_string().unwrap() == "function_call" {
                    let tool_call = ToolCall {
                        index: item_index,
                        id: fields[1].get_string(),
                        function: Function {
                            name: fields[2].get_string().unwrap(),
                            arguments: fields[3].get_string().unwrap(),
                        },
                        has_error: false,
                    };
                    Ok(ResponseEvent::FunctionCallStart(tool_call))
                } else {
                    Ok(ResponseEvent::OutputItemAdded)
                }
            }
            "response.content_part.added" => Ok(ResponseEvent::ContentPartAdded),
            "response.reasoning_text.done" | "response.reasoning_summary_text.done" => {
                Ok(ResponseEvent::ReasoningTextDone)
            }
            "response.reasoning_summary_part.added" => Ok(ResponseEvent::ReasoningSummaryAdded),
            "response.reasoning_summary_part.done" => Ok(ResponseEvent::ReasoningSummaryDone),
            "response.reasoning_text.delta" | "response.reasoning_summary_text.delta" => {
                let mut fields = [JsonField::new_string("delta")];
                autoparser(json, &mut fields)
                    .context("ResponseEvent reasoning_text.delta autoparser")?;
                Ok(ResponseEvent::ReasoningTextDelta(
                    fields[0].get_string().unwrap(),
                ))
            }
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
            "response.output_text.annotation.added" => {
                let mut fields = [JsonField::new_raw("annotation")];
                autoparser(json, &mut fields).context("ResponseEvent annotation autoparser")?;
                let a = Annotation::from_json(&fields[0].get_raw().unwrap())?;
                Ok(ResponseEvent::Annotation(a))
            }
            "response.function_call_arguments.delta" => {
                Ok(ResponseEvent::FunctionCallArgumentDelta)
            }
            "response.function_call_arguments.done" => {
                // We don't need to read the deltas, here we have full arguments
                let mut fields = [
                    JsonField::new_int("output_index"),
                    JsonField::new_simple_string("name"),
                    JsonField::new_string("arguments"),
                ];
                autoparser(json, &mut fields).context("ResponseEvent function_call autoparser")?;
                let tool_call = ToolCall {
                    index: fields[0].get_int().unwrap(),
                    id: None,
                    function: Function {
                        name: fields[1].get_string().unwrap(),
                        arguments: fields[2].get_string().unwrap(),
                    },
                    has_error: false,
                };
                Ok(ResponseEvent::FunctionCallArgumentsDone(tool_call))
            }
            "response.completed" => {
                let mut fields = [JsonField::new_raw("response")];
                autoparser(json, &mut fields)
                    .context("ResponseEvent response.completed autoparser")?;

                let resp_json = fields[0].get_raw().unwrap();
                let mut fields = [
                    JsonField::new_simple_string("model"),
                    JsonField::new_raw("usage"),
                ];
                autoparser(&resp_json, &mut fields)
                    .context("ResponseEvent response.completed autoparser 2")?;
                let model = fields[0].get_string().unwrap();
                let usage_json = fields[1].get_raw().unwrap();
                let usage = Usage::from_json(&usage_json)?;
                let u = stats::Usage {
                    cost_in_cents: Some(usage.cost as f64 * 100.0), // convert to cents
                    tokens: Some(usage.total_tokens),
                    web_search_requests: usage.web_search_requests,
                    tool_calls: usage.tool_calls_executed,
                };

                let st = stats::Stats {
                    metadata: stats::Metadata {
                        used_model: model,
                        provider: "".to_string(),
                    },
                    usage: Some(u),
                    // These two are calculated by extract_events
                    num_tokens: 0,
                    timing: None,
                };

                Ok(ResponseEvent::Completed(st))
            }
            "response.failed" => {
                let mut fields = [JsonField::new_raw("response")];
                autoparser(json, &mut fields)
                    .context("ResponseEvent response.failed autoparser")?;

                let resp_json = fields[0].get_raw().unwrap();
                let mut fields = [JsonField::new_raw("error")];
                autoparser(&resp_json, &mut fields)
                    .context("ResponseEvent response.failed autoparser 2")?;

                let error_json = fields[0].get_raw().unwrap();
                let mut fields = [
                    JsonField::new_simple_string("code"),
                    JsonField::new_string("message"),
                ];
                autoparser(&error_json, &mut fields)
                    .context("ResponseEvent response.failed autoparser 3")?;

                Ok(ResponseEvent::Failed {
                    code: fields[0].get_string().unwrap(),
                    message: fields[1].get_string().unwrap(),
                })
            }
            "response.incomplete" => {
                let mut fields = [JsonField::new_raw("response")];
                autoparser(json, &mut fields)
                    .context("ResponseEvent response.incomplete autoparser")?;

                let resp_json = fields[0].get_raw().unwrap();
                let mut fields = [JsonField::new_raw("incomplete_details")];
                autoparser(&resp_json, &mut fields)
                    .context("ResponseEvent response.incomplete 2")?;

                let incomplete_json = fields[0].get_raw().unwrap();
                let mut fields = [JsonField::new_simple_string("reason")];
                autoparser(&incomplete_json, &mut fields)
                    .context("ResponseEvent response.incomplete 3")?;

                Ok(ResponseEvent::Failed {
                    code: "Incomplete".to_string(),
                    message: fields[0].get_string().unwrap(),
                })
            }
            "error" => {
                let mut fields = [JsonField::new_raw("error")];
                autoparser(json, &mut fields).context("ResponseEvent error autoparser")?;

                let error_json = fields[0].get_raw().unwrap();
                let mut fields = [
                    JsonField::new_simple_string("code"),
                    JsonField::new_string("message"),
                ];
                autoparser(&error_json, &mut fields).context("ResponseEvent error autoparser 2")?;

                Ok(ResponseEvent::Failed {
                    code: fields[0].get_string().unwrap(),
                    message: fields[1].get_string().unwrap(),
                })
            }
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
        let ResponseEvent::Completed(st) = r else {
            panic!("Expected Completed");
        };
        assert_eq!(st.metadata.used_model.as_str(), "openai/gpt-oss-20b");
        assert_eq!(st.usage.and_then(|u| u.tokens), Some(118));
    }

    #[test]
    fn test_annotation_added() {
        let s = r#"{"type":"response.output_text.annotation.added","output_index":3,"item_id":"msg_tmp_32rlm6j37xg","content_index":0,"annotation_index":0,"annotation":{"type":"url_citation","url":"https://www.unfpa.org/data/world-population/WORLD","title":"WORLD Population 2025 - United Nations Population Fund | United Nations Population Fund","start_index":0,"end_index":0,"content":"WORLD Population 2025 - United Nations Population Fund | United Nations Population Fund\n\n# WORLD Population 2025 - United Nations Population Fund \n\n Total Population (2025): 8,232,000,000\n\n...\n\nTotal population: Estimated size of national populations at mid-year. ... Total population: World Population Prospects 2024 revision. United Nations Population Division, 2024."},"sequence_number":126}"#;
        let r = ResponseEvent::from_json(s).unwrap();
        let ResponseEvent::Annotation(a) = r else {
            panic!("Expected Annotation");
        };
        assert_eq!(
            a.citation_url(),
            "https://www.unfpa.org/data/world-population/WORLD"
        );
    }

    #[test]
    fn test_reasoning_summary() {
        let s1 = r#"{"type":"response.reasoning_summary_part.added","output_index":0,"item_id":"rs_f0dae57e-fc05-92fd-9721-19cc82d602f2","summary_index":0,"part":{"type":"summary_text","text":""},"sequence_number":3}"#;
        let s2 = r#"{"type":"response.reasoning_summary_text.delta","output_index":0,"item_id":"rs_f0dae57e-fc05-92fd-9721-19cc82d602f2","summary_index":0,"delta":"I'm mapping out a focused","sequence_number":4}"#;
        let s3 = r#"{"type":"response.reasoning_summary_text.done","output_index":0,"item_id":"rs_f0dae57e-fc05-92fd-9721-19cc82d602f2","summary_index":0,"text":"I'm mapping out a focused lesson plan for learning calculus. The goal is to suggest an outline rather than write full lessons.\n\nThe plan centers on building geometric intuition first. Limits, derivatives as slopes, and integrals as areas should be visualized before heavy computation. Practice should progress from simple to complex problems. The fundamental theorem of calculus will connect derivatives and integrals. Spaced review and applications will reinforce the concepts.","sequence_number":20}"#;
        let s4 = r#"{"type":"response.reasoning_summary_part.done","output_index":0,"item_id":"rs_f0dae57e-fc05-92fd-9721-19cc82d602f2","summary_index":0,"part":{"type":"summary_text","text":"I'm mapping out a focused lesson plan for learning calculus. The goal is to suggest an outline rather than write full lessons.\n\nThe plan centers on building geometric intuition first. Limits, derivatives as slopes, and integrals as areas should be visualized before heavy computation. Practice should progress from simple to complex problems. The fundamental theorem of calculus will connect derivatives and integrals. Spaced review and applications will reinforce the concepts."},"sequence_number":21}"#;

        let _ = ResponseEvent::from_json(s1).unwrap();
        let r = ResponseEvent::from_json(s2).unwrap();
        let ResponseEvent::ReasoningTextDelta(delta) = r else {
            panic!("Expected ReasoningTextDelta");
        };
        assert_eq!(delta.as_str(), "I'm mapping out a focused");
        let _ = ResponseEvent::from_json(s3).unwrap();
        let _ = ResponseEvent::from_json(s4).unwrap();
    }
}
