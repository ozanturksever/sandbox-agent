use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

use crate::{
    ContentPart, ErrorData, EventConversion, ItemDeltaData, ItemEventData, ItemKind, ItemRole,
    ItemStatus, QuestionEventData, QuestionStatus, SessionEndReason, SessionEndedData,
    SessionStartedData, TerminatedBy, UniversalEventData, UniversalEventType, UniversalItem,
};

static TEMP_ID: AtomicU64 = AtomicU64::new(1);

fn next_temp_id(prefix: &str) -> String {
    let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_{id}")
}

/// Convert a Codebuff PrintModeEvent (as JSON Value) to universal schema events.
pub fn event_to_universal(event: &Value) -> Result<Vec<EventConversion>, String> {
    let event_type = event.get("type").and_then(Value::as_str).unwrap_or("");

    let mut conversions = match event_type {
        "start" => start_event_to_universal(event),
        "text" => text_event_to_universal(event),
        "reasoning_delta" => reasoning_delta_to_universal(event),
        "tool_call" => tool_call_to_universal(event),
        "tool_result" => tool_result_to_universal(event),
        "tool_progress" => tool_progress_to_universal(event),
        "subagent_start" => subagent_start_to_universal(event),
        "subagent_finish" => subagent_finish_to_universal(event),
        "subagent_chunk" => Vec::new(), // Subagent text chunks are handled by text events
        "reasoning_chunk" => Vec::new(), // Reasoning chunks are handled by reasoning_delta
        "error" => error_event_to_universal(event),
        "finish" => finish_event_to_universal(event),
        "download" => Vec::new(), // Download status events are informational only
        "" => Vec::new(), // Ignore events without a type (e.g., malformed or metadata lines)
        // For unknown event types, return empty rather than error - this allows graceful handling
        // of new event types that may be added to Codebuff in the future
        _ => Vec::new(),
    };

    for conversion in &mut conversions {
        conversion.raw = Some(event.clone());
    }

    Ok(conversions)
}

fn start_event_to_universal(event: &Value) -> Vec<EventConversion> {
    let agent_id = event.get("agentId").and_then(Value::as_str);
    let model = event.get("model").and_then(Value::as_str);
    let message_history_length = event
        .get("messageHistoryLength")
        .and_then(Value::as_i64)
        .unwrap_or(0);

    let mut metadata = serde_json::Map::new();
    metadata.insert("agent".to_string(), Value::String("codebuff".to_string()));
    if let Some(agent_id) = agent_id {
        metadata.insert("agentId".to_string(), Value::String(agent_id.to_string()));
    }
    if let Some(model) = model {
        metadata.insert("model".to_string(), Value::String(model.to_string()));
    }
    metadata.insert(
        "messageHistoryLength".to_string(),
        Value::Number(message_history_length.into()),
    );

    vec![EventConversion::new(
        UniversalEventType::SessionStarted,
        UniversalEventData::SessionStarted(SessionStartedData {
            metadata: Some(Value::Object(metadata)),
        }),
    )]
}

fn text_event_to_universal(event: &Value) -> Vec<EventConversion> {
    let text = event
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let agent_id = event
        .get("agentId")
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    if text.is_empty() {
        return Vec::new();
    }

    let native_item_id = agent_id
        .clone()
        .unwrap_or_else(|| next_temp_id("codebuff_text"));

    vec![EventConversion::new(
        UniversalEventType::ItemDelta,
        UniversalEventData::ItemDelta(ItemDeltaData {
            item_id: String::new(),
            native_item_id: Some(native_item_id),
            delta: text,
        }),
    )]
}

fn reasoning_delta_to_universal(event: &Value) -> Vec<EventConversion> {
    let text = event
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let run_id = event
        .get("runId")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .unwrap_or_else(|| next_temp_id("codebuff_reasoning"));

    if text.is_empty() {
        return Vec::new();
    }

    // Emit reasoning as a delta event with the reasoning content
    // The item will be completed when the message finishes
    vec![EventConversion::new(
        UniversalEventType::ItemDelta,
        UniversalEventData::ItemDelta(ItemDeltaData {
            item_id: String::new(),
            native_item_id: Some(format!("reasoning_{run_id}")),
            delta: text,
        }),
    )]
}

fn tool_call_to_universal(event: &Value) -> Vec<EventConversion> {
    let tool_call_id = event
        .get("toolCallId")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .unwrap_or_else(|| next_temp_id("codebuff_tool"));
    let tool_name = event
        .get("toolName")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let input = event.get("input").cloned().unwrap_or(Value::Null);
    let _agent_id = event.get("agentId").and_then(Value::as_str);
    let parent_agent_id = event
        .get("parentAgentId")
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    let arguments = serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());

    // Check if this is an ask_user tool (question)
    let is_question_tool = matches!(
        tool_name.as_str(),
        "ask_user" | "AskUser" | "ask-user" | "askUser"
    );

    let mut conversions = Vec::new();

    // If it's a question tool, emit a question.requested event
    if is_question_tool {
        if let Some(question) = question_from_ask_user_input(&input, tool_call_id.clone()) {
            conversions.push(EventConversion::new(
                UniversalEventType::QuestionRequested,
                UniversalEventData::Question(question),
            ));
        }
    }

    let tool_item = UniversalItem {
        item_id: String::new(),
        native_item_id: Some(tool_call_id.clone()),
        parent_id: parent_agent_id,
        kind: ItemKind::ToolCall,
        role: Some(ItemRole::Assistant),
        content: vec![ContentPart::ToolCall {
            name: tool_name,
            arguments,
            call_id: tool_call_id,
        }],
        status: ItemStatus::Completed,
    };

    conversions.extend(item_events(tool_item, true));
    conversions
}

fn tool_result_to_universal(event: &Value) -> Vec<EventConversion> {
    let tool_call_id = event
        .get("toolCallId")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .unwrap_or_else(|| next_temp_id("codebuff_tool"));
    let tool_name = event
        .get("toolName")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let output = event.get("output").cloned().unwrap_or(Value::Null);
    let parent_agent_id = event
        .get("parentAgentId")
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    let output_text = serde_json::to_string(&output).unwrap_or_default();

    // Check if this is an ask_user tool result (question resolved)
    let is_question_tool = matches!(
        tool_name,
        "ask_user" | "AskUser" | "ask-user" | "askUser"
    );

    let mut conversions = Vec::new();

    // If it's a question tool result, emit question.resolved
    if is_question_tool {
        let response = extract_question_response(&output);
        conversions.push(EventConversion::new(
            UniversalEventType::QuestionResolved,
            UniversalEventData::Question(QuestionEventData {
                question_id: tool_call_id.clone(),
                prompt: String::new(),
                options: Vec::new(),
                response,
                status: QuestionStatus::Answered,
            }),
        ));
    }

    let tool_item = UniversalItem {
        item_id: next_temp_id("codebuff_tool_result"),
        native_item_id: Some(tool_call_id.clone()),
        parent_id: parent_agent_id,
        kind: ItemKind::ToolResult,
        role: Some(ItemRole::Tool),
        content: vec![ContentPart::ToolResult {
            call_id: tool_call_id,
            output: output_text,
        }],
        status: ItemStatus::Completed,
    };

    conversions.extend(item_events(tool_item, true));
    conversions
}

fn tool_progress_to_universal(event: &Value) -> Vec<EventConversion> {
    let tool_call_id = event
        .get("toolCallId")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .unwrap_or_else(|| next_temp_id("codebuff_tool"));
    let output = event
        .get("output")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    if output.is_empty() {
        return Vec::new();
    }

    vec![EventConversion::new(
        UniversalEventType::ItemDelta,
        UniversalEventData::ItemDelta(ItemDeltaData {
            item_id: String::new(),
            native_item_id: Some(tool_call_id),
            delta: output,
        }),
    )]
}

fn subagent_start_to_universal(event: &Value) -> Vec<EventConversion> {
    let agent_id = event
        .get("agentId")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .unwrap_or_else(|| next_temp_id("codebuff_subagent"));
    let agent_type = event
        .get("agentType")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let display_name = event
        .get("displayName")
        .and_then(Value::as_str)
        .unwrap_or(&agent_type)
        .to_string();
    let parent_agent_id = event
        .get("parentAgentId")
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    let model = event.get("model").and_then(Value::as_str);
    let prompt = event.get("prompt").and_then(Value::as_str);

    let mut detail = display_name.clone();
    if let Some(model) = model {
        detail = format!("{detail} ({model})");
    }
    if let Some(prompt) = prompt {
        let preview = if prompt.len() > 50 {
            format!("{}...", &prompt[..50])
        } else {
            prompt.to_string()
        };
        detail = format!("{detail}: {preview}");
    }

    let item = UniversalItem {
        item_id: String::new(),
        native_item_id: Some(agent_id),
        parent_id: parent_agent_id,
        kind: ItemKind::Status,
        role: Some(ItemRole::Assistant),
        content: vec![ContentPart::Status {
            label: format!("subagent:{agent_type}"),
            detail: Some(detail),
        }],
        status: ItemStatus::InProgress,
    };

    vec![EventConversion::new(
        UniversalEventType::ItemStarted,
        UniversalEventData::Item(ItemEventData { item }),
    )]
}

fn subagent_finish_to_universal(event: &Value) -> Vec<EventConversion> {
    let agent_id = event
        .get("agentId")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .unwrap_or_else(|| next_temp_id("codebuff_subagent"));
    let agent_type = event
        .get("agentType")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let display_name = event
        .get("displayName")
        .and_then(Value::as_str)
        .unwrap_or(&agent_type)
        .to_string();
    let parent_agent_id = event
        .get("parentAgentId")
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    let item = UniversalItem {
        item_id: String::new(),
        native_item_id: Some(agent_id),
        parent_id: parent_agent_id,
        kind: ItemKind::Status,
        role: Some(ItemRole::Assistant),
        content: vec![ContentPart::Status {
            label: format!("subagent:{agent_type}"),
            detail: Some(format!("{display_name} completed")),
        }],
        status: ItemStatus::Completed,
    };

    vec![EventConversion::new(
        UniversalEventType::ItemCompleted,
        UniversalEventData::Item(ItemEventData { item }),
    )]
}

fn error_event_to_universal(event: &Value) -> Vec<EventConversion> {
    let message = event
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Unknown error")
        .to_string();

    vec![EventConversion::new(
        UniversalEventType::Error,
        UniversalEventData::Error(ErrorData {
            message,
            code: Some("codebuff".to_string()),
            details: Some(event.clone()),
        }),
    )]
}

fn finish_event_to_universal(_event: &Value) -> Vec<EventConversion> {
    vec![EventConversion::new(
        UniversalEventType::SessionEnded,
        UniversalEventData::SessionEnded(SessionEndedData {
            reason: SessionEndReason::Completed,
            terminated_by: TerminatedBy::Agent,
            message: None,
            exit_code: None,
            stderr: None,
        }),
    )]
}

fn item_events(item: UniversalItem, synthetic_start: bool) -> Vec<EventConversion> {
    let mut events = Vec::new();
    if synthetic_start {
        let mut started_item = item.clone();
        started_item.status = ItemStatus::InProgress;
        events.push(
            EventConversion::new(
                UniversalEventType::ItemStarted,
                UniversalEventData::Item(ItemEventData { item: started_item }),
            )
            .synthetic(),
        );
    }
    events.push(EventConversion::new(
        UniversalEventType::ItemCompleted,
        UniversalEventData::Item(ItemEventData { item }),
    ));
    events
}

fn question_from_ask_user_input(input: &Value, tool_id: String) -> Option<QuestionEventData> {
    // Try to extract questions array (matching ask_user tool schema)
    if let Some(questions) = input.get("questions").and_then(Value::as_array) {
        if let Some(first) = questions.first() {
            let prompt = first
                .get("question")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let options = first
                .get("options")
                .and_then(Value::as_array)
                .map(|opts| {
                    opts.iter()
                        .filter_map(|opt| opt.get("label").and_then(Value::as_str))
                        .map(|label| label.to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            if !prompt.is_empty() {
                return Some(QuestionEventData {
                    question_id: tool_id,
                    prompt,
                    options,
                    response: None,
                    status: QuestionStatus::Requested,
                });
            }
        }
    }

    // Try single question format
    let prompt = input
        .get("question")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if prompt.is_empty() {
        return None;
    }

    let options = input
        .get("options")
        .and_then(Value::as_array)
        .map(|opts| {
            opts.iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Some(QuestionEventData {
        question_id: tool_id,
        prompt,
        options,
        response: None,
        status: QuestionStatus::Requested,
    })
}

fn extract_question_response(output: &Value) -> Option<String> {
    // Try to extract response from tool result output
    if let Some(arr) = output.as_array() {
        for item in arr {
            if let Some(value) = item.get("value") {
                if let Some(s) = value.as_str() {
                    return Some(s.to_string());
                }
                if let Some(obj) = value.as_object() {
                    if let Some(response) = obj.get("response").and_then(Value::as_str) {
                        return Some(response.to_string());
                    }
                    if let Some(answer) = obj.get("answer").and_then(Value::as_str) {
                        return Some(answer.to_string());
                    }
                }
            }
        }
    }
    None
}
