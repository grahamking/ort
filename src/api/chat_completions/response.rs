//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

use core::cmp::max;

extern crate alloc;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::{
    ChatCompletionsResponse, Response, ThinkEvent,
    common::{data::ToolCall, stats},
    time,
};

pub fn extract_events(
    mut v: ChatCompletionsResponse,
    tsc_calibration: Option<time::TscCalibration>,
    queue: &mut Vec<Response>,
    stats: &mut stats::Stats,
    pending_tool_calls: &mut Vec<ToolCall>,
) {
    // Handle last message which contains the "usage" key
    // Do this before getting choices because it's empty on last message.
    if let Some(usage) = &v.usage {
        let u = stats::Usage {
            cost_in_cents: Some(usage.cost as f64 * 100.0), // convert to cents
            tokens: Some(usage.total_tokens),
            web_search_requests: usage.web_search_requests,
            tool_calls: usage.tool_calls_executed,
        };
        queue.push(Response::Usage(u));

        // Chat Completions includes this on every message. We only
        // want it once, so use the last message, the Usage one.
        let m = stats::Metadata {
            provider: v.provider.clone().unwrap_or_default(),
            used_model: v.model.clone().unwrap_or_default(),
        };
        queue.push(Response::Metadata(m));

        // We now have everything we need to calculate stats.
        // Usage always comes last.
        queue.push(Response::Stats(stats.clone()));
    }

    if let Some(error) = &v.error {
        queue.push(Response::Error(error.message().to_string()));
    };

    // Standard OpenAI stream delta shape
    let Some(choice) = v.choices.pop() else {
        return;
    };

    let has_reasoning = choice
        .delta
        .reasoning
        .as_ref()
        .map(|x| !x.is_empty())
        .unwrap_or(false);
    let has_reasoning_details = choice.delta.reasoning_details.is_some();
    let content = choice.delta.text();
    let has_content = content.map(|x| !x.is_empty()).unwrap_or(false);
    let has_tool_calls = !choice.delta.tool_calls.is_empty();
    let has_annotations = !choice.delta.annotations.is_empty();
    let mut is_finished = false;
    if let Some(finish_reason) = &choice.finish_reason {
        if let Some(tc) = tsc_calibration {
            record_final_timing(tc, stats);
        }
        queue.push(Response::Finish(finish_reason.clone()));
        is_finished = true;
    }
    if !(has_reasoning
        || has_reasoning_details
        || has_content
        || has_tool_calls
        || has_annotations
        || is_finished)
    {
        return;
    }

    // Record time to first token
    if let Some(t) = stats.timing.as_mut()
        && t.time_to_first_token.is_none()
    {
        let first_token = time::Ticks::now();
        t.time_to_first_token =
            tsc_calibration.map(|tc| time::elapsed_duration(t.start, first_token, tc));
        t.token_stream_start = Some(time::Ticks::now());
    }

    // Handle tool calls
    // TODO: Consider copying and ownership
    // TODO: If we have lots of tool calls (> 10) switch
    // to a different data structure for pending_tool_calls.
    if has_tool_calls {
        for tool_call in &choice.delta.tool_calls {
            match pending_tool_calls
                .iter_mut()
                .find(|item| item.index == tool_call.index)
            {
                Some(pending) => {
                    pending.update_from(tool_call);
                }
                None => {
                    // We don't have a pending one either because this is a new one,
                    // or we hit an error and have to skip the call.
                    // A new one has an id.
                    if tool_call.id.is_some() {
                        pending_tool_calls.push(tool_call.clone());
                    }
                }
            }
        }
    }

    // Handle reasoning content
    if let Some(reasoning_content) = choice.delta.reasoning.as_ref()
        && !reasoning_content.is_empty()
    {
        stats.num_tokens += 1;
        let r_event = Response::Think(ThinkEvent::Content(reasoning_content.to_string()));
        queue.push(r_event);
    }
    if let Some(reasoning_details) = choice.delta.reasoning_details.as_ref() {
        let r_event = Response::Think(ThinkEvent::Details(reasoning_details.to_string()));
        queue.push(r_event);
    }

    // Handle annotations
    for a in &choice.delta.annotations {
        queue.push(Response::Annotation(a.clone()));
    }

    // Handle regular content
    if let Some(content) = content
        && !content.is_empty()
    {
        stats.num_tokens += 1;
        let r_event = Response::Content(content.to_string());
        queue.push(r_event);
    }

    if choice.is_tool_call_finish() && !pending_tool_calls.is_empty() {
        let event = Response::ToolCalls(core::mem::take(pending_tool_calls));
        queue.push(event);
    }
}

fn record_final_timing(tsc_calibration: time::TscCalibration, stats: &mut stats::Stats) {
    let Some(t) = stats.timing.as_mut() else {
        unreachable!("No data received on socket");
    };
    let now = time::Ticks::now();
    t.elapsed_time = time::elapsed_duration(t.start, now, tsc_calibration);
    if let Some(token_stream_start) = t.token_stream_start {
        let stream_elapsed_time = time::elapsed_duration(token_stream_start, now, tsc_calibration);
        t.inter_token_latency_ms =
            stream_elapsed_time.as_millis() / max(stats.num_tokens, 1) as u128;
    }
}
