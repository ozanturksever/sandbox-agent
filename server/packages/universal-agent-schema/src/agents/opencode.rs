use serde_json::Value;

use crate::opencode as schema;
use crate::{
    ContentPart, EventConversion, ItemDeltaData, ItemEventData, ItemKind, ItemRole, ItemStatus,
    PermissionEventData, PermissionStatus, QuestionEventData, QuestionStatus, ReasoningVisibility,
    SessionStartedData, TurnEventData, TurnPhase, UniversalEventData, UniversalEventType,
    UniversalItem,
};

pub fn event_to_universal(event: &schema::Event) -> Result<Vec<EventConversion>, String> {
    let raw = serde_json::to_value(event).ok();
    match event {
        schema::Event::MessageUpdated(updated) => {
            let schema::EventMessageUpdated {
                properties,
                type_: _,
            } = updated;
            let schema::EventMessageUpdatedProperties { info } = properties;
            let (mut item, completed, session_id) = message_to_item(info);
            item.status = if completed {
                ItemStatus::Completed
            } else {
                ItemStatus::InProgress
            };
            let event_type = if completed {
                UniversalEventType::ItemCompleted
            } else {
                UniversalEventType::ItemStarted
            };
            let conversion =
                EventConversion::new(event_type, UniversalEventData::Item(ItemEventData { item }))
                    .with_native_session(session_id)
                    .with_raw(raw);
            Ok(vec![conversion])
        }
        schema::Event::MessagePartUpdated(updated) => {
            let schema::EventMessagePartUpdated {
                properties,
                type_: _,
            } = updated;
            let schema::EventMessagePartUpdatedProperties { part, delta } = properties;
            let mut events = Vec::new();
            let (session_id, message_id) = part_session_message(part);

            match part {
                schema::Part::TextPart(text_part) => {
                    let schema::TextPart { text, .. } = text_part;
                    let delta_text = delta.as_ref().unwrap_or(&text).clone();
                    let stub = stub_message_item(&message_id, ItemRole::Assistant);
                    events.push(
                        EventConversion::new(
                            UniversalEventType::ItemStarted,
                            UniversalEventData::Item(ItemEventData { item: stub }),
                        )
                        .synthetic()
                        .with_raw(raw.clone()),
                    );
                    events.push(
                        EventConversion::new(
                            UniversalEventType::ItemDelta,
                            UniversalEventData::ItemDelta(ItemDeltaData {
                                item_id: String::new(),
                                native_item_id: Some(message_id.clone()),
                                delta: delta_text,
                            }),
                        )
                        .with_native_session(session_id.clone())
                        .with_raw(raw.clone()),
                    );
                }
                schema::Part::ReasoningPart(reasoning_part) => {
                    let reasoning_text = delta
                        .as_ref()
                        .cloned()
                        .unwrap_or_else(|| reasoning_part.text.clone());
                    let reasoning_id = reasoning_part.id.clone();
                    let mut started = stub_message_item(&reasoning_id, ItemRole::Assistant);
                    started.parent_id = Some(message_id.clone());
                    let completed = UniversalItem {
                        item_id: String::new(),
                        native_item_id: Some(reasoning_id),
                        parent_id: Some(message_id.clone()),
                        kind: ItemKind::Message,
                        role: Some(ItemRole::Assistant),
                        content: vec![ContentPart::Reasoning {
                            text: reasoning_text,
                            visibility: ReasoningVisibility::Public,
                        }],
                        status: ItemStatus::Completed,
                    };
                    events.push(
                        EventConversion::new(
                            UniversalEventType::ItemStarted,
                            UniversalEventData::Item(ItemEventData { item: started }),
                        )
                        .synthetic()
                        .with_raw(raw.clone()),
                    );
                    events.push(
                        EventConversion::new(
                            UniversalEventType::ItemCompleted,
                            UniversalEventData::Item(ItemEventData { item: completed }),
                        )
                        .with_native_session(session_id.clone())
                        .with_raw(raw.clone()),
                    );
                }
                schema::Part::FilePart(file_part) => {
                    let file_content = file_part_to_content(file_part);
                    let item = UniversalItem {
                        item_id: String::new(),
                        native_item_id: Some(message_id.clone()),
                        parent_id: None,
                        kind: ItemKind::Message,
                        role: Some(ItemRole::Assistant),
                        content: vec![file_content],
                        status: ItemStatus::Completed,
                    };
                    events.push(
                        EventConversion::new(
                            UniversalEventType::ItemCompleted,
                            UniversalEventData::Item(ItemEventData { item }),
                        )
                        .with_native_session(session_id.clone())
                        .with_raw(raw.clone()),
                    );
                }
                schema::Part::ToolPart(tool_part) => {
                    let tool_events = tool_part_to_events(&tool_part, &message_id);
                    for event in tool_events {
                        events.push(
                            event
                                .with_native_session(session_id.clone())
                                .with_raw(raw.clone()),
                        );
                    }
                }
                schema::Part::SubtaskPart(subtask_part) => {
                    let detail = serde_json::to_string(subtask_part)
                        .unwrap_or_else(|_| "subtask".to_string());
                    let item = status_item("subtask", Some(detail));
                    events.push(
                        EventConversion::new(
                            UniversalEventType::ItemCompleted,
                            UniversalEventData::Item(ItemEventData { item }),
                        )
                        .with_native_session(session_id.clone())
                        .with_raw(raw.clone()),
                    );
                }
                schema::Part::StepStartPart(_)
                | schema::Part::StepFinishPart(_)
                | schema::Part::SnapshotPart(_)
                | schema::Part::PatchPart(_)
                | schema::Part::AgentPart(_)
                | schema::Part::RetryPart(_)
                | schema::Part::CompactionPart(_) => {
                    let detail = serde_json::to_string(part).unwrap_or_else(|_| "part".to_string());
                    let item = status_item("part.updated", Some(detail));
                    events.push(
                        EventConversion::new(
                            UniversalEventType::ItemCompleted,
                            UniversalEventData::Item(ItemEventData { item }),
                        )
                        .with_native_session(session_id.clone())
                        .with_raw(raw.clone()),
                    );
                }
            }

            Ok(events)
        }
        schema::Event::QuestionAsked(asked) => {
            let schema::EventQuestionAsked {
                properties,
                type_: _,
            } = asked;
            let question = question_from_opencode(properties);
            let conversion = EventConversion::new(
                UniversalEventType::QuestionRequested,
                UniversalEventData::Question(question),
            )
            .with_native_session(Some(properties.session_id.to_string()))
            .with_raw(raw);
            Ok(vec![conversion])
        }
        schema::Event::PermissionAsked(asked) => {
            let schema::EventPermissionAsked {
                properties,
                type_: _,
            } = asked;
            let permission = permission_from_opencode(properties);
            let conversion = EventConversion::new(
                UniversalEventType::PermissionRequested,
                UniversalEventData::Permission(permission),
            )
            .with_native_session(Some(properties.session_id.to_string()))
            .with_raw(raw);
            Ok(vec![conversion])
        }
        schema::Event::SessionCreated(created) => {
            let schema::EventSessionCreated {
                properties,
                type_: _,
            } = created;
            let metadata = serde_json::to_value(&properties.info).ok();
            let conversion = EventConversion::new(
                UniversalEventType::SessionStarted,
                UniversalEventData::SessionStarted(SessionStartedData { metadata }),
            )
            .with_native_session(Some(properties.info.id.to_string()))
            .with_raw(raw);
            Ok(vec![conversion])
        }
        schema::Event::SessionStatus(status) => {
            let schema::EventSessionStatus {
                properties,
                type_: _,
            } = status;
            let status_type = serde_json::to_value(&properties.status)
                .ok()
                .and_then(|value| {
                    value
                        .get("type")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                });
            let detail =
                serde_json::to_string(&properties.status).unwrap_or_else(|_| "status".to_string());
            let item = status_item("session.status", Some(detail));
            let mut events = vec![EventConversion::new(
                UniversalEventType::ItemCompleted,
                UniversalEventData::Item(ItemEventData { item }),
            )
            .with_native_session(Some(properties.session_id.clone()))
            .with_raw(raw.clone())];

            if matches!(status_type.as_deref(), Some("busy" | "idle")) {
                let (event_type, phase) = if status_type.as_deref() == Some("busy") {
                    (UniversalEventType::TurnStarted, TurnPhase::Started)
                } else {
                    (UniversalEventType::TurnEnded, TurnPhase::Ended)
                };
                events.push(
                    EventConversion::new(
                        event_type,
                        UniversalEventData::Turn(TurnEventData {
                            phase,
                            turn_id: None,
                            metadata: Some(
                                serde_json::to_value(&properties.status).unwrap_or(Value::Null),
                            ),
                        }),
                    )
                    .with_native_session(Some(properties.session_id.clone()))
                    .with_raw(raw),
                );
            }
            Ok(events)
        }
        schema::Event::SessionIdle(idle) => {
            let schema::EventSessionIdle {
                properties,
                type_: _,
            } = idle;
            let conversion = EventConversion::new(
                UniversalEventType::TurnEnded,
                UniversalEventData::Turn(TurnEventData {
                    phase: TurnPhase::Ended,
                    turn_id: None,
                    metadata: None,
                }),
            )
            .with_native_session(Some(properties.session_id.clone()))
            .with_raw(raw);
            Ok(vec![conversion])
        }
        schema::Event::SessionError(error) => {
            let schema::EventSessionError {
                properties,
                type_: _,
            } = error;
            let detail = serde_json::to_string(&properties.error)
                .unwrap_or_else(|_| "session error".to_string());
            let item = status_item("session.error", Some(detail));
            let conversion = EventConversion::new(
                UniversalEventType::ItemCompleted,
                UniversalEventData::Item(ItemEventData { item }),
            )
            .with_native_session(properties.session_id.clone())
            .with_raw(raw);
            Ok(vec![conversion])
        }
        _ => Err("unsupported opencode event".to_string()),
    }
}

fn message_to_item(message: &schema::Message) -> (UniversalItem, bool, Option<String>) {
    match message {
        schema::Message::UserMessage(user) => {
            let schema::UserMessage {
                id,
                session_id,
                role: _,
                ..
            } = user;
            (
                UniversalItem {
                    item_id: String::new(),
                    native_item_id: Some(id.clone()),
                    parent_id: None,
                    kind: ItemKind::Message,
                    role: Some(ItemRole::User),
                    content: Vec::new(),
                    status: ItemStatus::Completed,
                },
                true,
                Some(session_id.clone()),
            )
        }
        schema::Message::AssistantMessage(assistant) => {
            let schema::AssistantMessage {
                id,
                session_id,
                time,
                ..
            } = assistant;
            let completed = time.completed.is_some();
            (
                UniversalItem {
                    item_id: String::new(),
                    native_item_id: Some(id.clone()),
                    parent_id: None,
                    kind: ItemKind::Message,
                    role: Some(ItemRole::Assistant),
                    content: Vec::new(),
                    status: if completed {
                        ItemStatus::Completed
                    } else {
                        ItemStatus::InProgress
                    },
                },
                completed,
                Some(session_id.clone()),
            )
        }
    }
}

fn part_session_message(part: &schema::Part) -> (Option<String>, String) {
    match part {
        schema::Part::TextPart(text_part) => (
            Some(text_part.session_id.clone()),
            text_part.message_id.clone(),
        ),
        schema::Part::SubtaskPart(subtask_part) => (
            Some(subtask_part.session_id.clone()),
            subtask_part.message_id.clone(),
        ),
        schema::Part::ReasoningPart(reasoning_part) => (
            Some(reasoning_part.session_id.clone()),
            reasoning_part.message_id.clone(),
        ),
        schema::Part::FilePart(file_part) => (
            Some(file_part.session_id.clone()),
            file_part.message_id.clone(),
        ),
        schema::Part::ToolPart(tool_part) => (
            Some(tool_part.session_id.clone()),
            tool_part.message_id.clone(),
        ),
        schema::Part::StepStartPart(step_part) => (
            Some(step_part.session_id.clone()),
            step_part.message_id.clone(),
        ),
        schema::Part::StepFinishPart(step_part) => (
            Some(step_part.session_id.clone()),
            step_part.message_id.clone(),
        ),
        schema::Part::SnapshotPart(snapshot_part) => (
            Some(snapshot_part.session_id.clone()),
            snapshot_part.message_id.clone(),
        ),
        schema::Part::PatchPart(patch_part) => (
            Some(patch_part.session_id.clone()),
            patch_part.message_id.clone(),
        ),
        schema::Part::AgentPart(agent_part) => (
            Some(agent_part.session_id.clone()),
            agent_part.message_id.clone(),
        ),
        schema::Part::RetryPart(retry_part) => (
            Some(retry_part.session_id.clone()),
            retry_part.message_id.clone(),
        ),
        schema::Part::CompactionPart(compaction_part) => (
            Some(compaction_part.session_id.clone()),
            compaction_part.message_id.clone(),
        ),
    }
}

fn stub_message_item(message_id: &str, role: ItemRole) -> UniversalItem {
    UniversalItem {
        item_id: String::new(),
        native_item_id: Some(message_id.to_string()),
        parent_id: None,
        kind: ItemKind::Message,
        role: Some(role),
        content: Vec::new(),
        status: ItemStatus::InProgress,
    }
}

fn tool_part_to_events(tool_part: &schema::ToolPart, message_id: &str) -> Vec<EventConversion> {
    let schema::ToolPart {
        call_id,
        state,
        tool,
        ..
    } = tool_part;
    let mut events = Vec::new();
    match state {
        schema::ToolState::Pending(state) => {
            let arguments = serde_json::to_string(&Value::Object(state.input.clone()))
                .unwrap_or_else(|_| "{}".to_string());
            let item = UniversalItem {
                item_id: String::new(),
                native_item_id: Some(call_id.clone()),
                parent_id: Some(message_id.to_string()),
                kind: ItemKind::ToolCall,
                role: Some(ItemRole::Assistant),
                content: vec![ContentPart::ToolCall {
                    name: tool.clone(),
                    arguments,
                    call_id: call_id.clone(),
                }],
                status: ItemStatus::InProgress,
            };
            events.push(EventConversion::new(
                UniversalEventType::ItemStarted,
                UniversalEventData::Item(ItemEventData { item }),
            ));
        }
        schema::ToolState::Running(state) => {
            let arguments = serde_json::to_string(&Value::Object(state.input.clone()))
                .unwrap_or_else(|_| "{}".to_string());
            let item = UniversalItem {
                item_id: String::new(),
                native_item_id: Some(call_id.clone()),
                parent_id: Some(message_id.to_string()),
                kind: ItemKind::ToolCall,
                role: Some(ItemRole::Assistant),
                content: vec![ContentPart::ToolCall {
                    name: tool.clone(),
                    arguments,
                    call_id: call_id.clone(),
                }],
                status: ItemStatus::InProgress,
            };
            events.push(EventConversion::new(
                UniversalEventType::ItemStarted,
                UniversalEventData::Item(ItemEventData { item }),
            ));
        }
        schema::ToolState::Completed(state) => {
            let output = state.output.clone();
            let mut content = vec![ContentPart::ToolResult {
                call_id: call_id.clone(),
                output,
            }];
            for attachment in &state.attachments {
                content.push(file_part_to_content(attachment));
            }
            let item = UniversalItem {
                item_id: String::new(),
                native_item_id: Some(call_id.clone()),
                parent_id: Some(message_id.to_string()),
                kind: ItemKind::ToolResult,
                role: Some(ItemRole::Tool),
                content,
                status: ItemStatus::Completed,
            };
            events.push(EventConversion::new(
                UniversalEventType::ItemCompleted,
                UniversalEventData::Item(ItemEventData { item }),
            ));
        }
        schema::ToolState::Error(state) => {
            let output = state.error.clone();
            let item = UniversalItem {
                item_id: String::new(),
                native_item_id: Some(call_id.clone()),
                parent_id: Some(message_id.to_string()),
                kind: ItemKind::ToolResult,
                role: Some(ItemRole::Tool),
                content: vec![ContentPart::ToolResult {
                    call_id: call_id.clone(),
                    output,
                }],
                status: ItemStatus::Failed,
            };
            events.push(EventConversion::new(
                UniversalEventType::ItemCompleted,
                UniversalEventData::Item(ItemEventData { item }),
            ));
        }
    }
    events
}

fn file_part_to_content(file_part: &schema::FilePart) -> ContentPart {
    let path = file_part.url.clone();
    let action = if file_part.mime.starts_with("image/") {
        crate::FileAction::Read
    } else {
        crate::FileAction::Read
    };
    ContentPart::FileRef {
        path,
        action,
        diff: None,
    }
}

fn status_item(label: &str, detail: Option<String>) -> UniversalItem {
    UniversalItem {
        item_id: String::new(),
        native_item_id: None,
        parent_id: None,
        kind: ItemKind::Status,
        role: Some(ItemRole::System),
        content: vec![ContentPart::Status {
            label: label.to_string(),
            detail,
        }],
        status: ItemStatus::Completed,
    }
}

fn question_from_opencode(request: &schema::QuestionRequest) -> QuestionEventData {
    let prompt = request
        .questions
        .first()
        .map(|q| q.question.clone())
        .unwrap_or_default();
    let options = request
        .questions
        .first()
        .map(|q| {
            q.options
                .iter()
                .map(|opt| opt.label.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    QuestionEventData {
        question_id: request.id.clone().into(),
        prompt,
        options,
        response: None,
        status: QuestionStatus::Requested,
    }
}

fn permission_from_opencode(request: &schema::PermissionRequest) -> PermissionEventData {
    PermissionEventData {
        permission_id: request.id.clone().into(),
        action: request.permission.clone(),
        status: PermissionStatus::Requested,
        metadata: serde_json::to_value(request).ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasoning_part_updates_stay_typed_not_text_delta() {
        let event = schema::Event::MessagePartUpdated(schema::EventMessagePartUpdated {
            properties: schema::EventMessagePartUpdatedProperties {
                delta: Some("Preparing friendly brief response".to_string()),
                part: schema::Part::ReasoningPart(schema::ReasoningPart {
                    id: "part_reason_1".to_string(),
                    message_id: "msg_1".to_string(),
                    metadata: serde_json::Map::new(),
                    session_id: "ses_1".to_string(),
                    text: "Preparing".to_string(),
                    time: schema::ReasoningPartTime {
                        end: None,
                        start: 0.0,
                    },
                    type_: "reasoning".to_string(),
                }),
            },
            type_: "message.part.updated".to_string(),
        });

        let converted = event_to_universal(&event).expect("conversion succeeds");
        assert_eq!(converted.len(), 2);
        assert!(converted
            .iter()
            .all(|entry| entry.event_type != UniversalEventType::ItemDelta));

        let completed = converted
            .iter()
            .find(|entry| entry.event_type == UniversalEventType::ItemCompleted)
            .expect("item.completed exists");
        let UniversalEventData::Item(ItemEventData { item }) = &completed.data else {
            panic!("expected item payload");
        };
        assert_eq!(item.native_item_id.as_deref(), Some("part_reason_1"));
        assert!(matches!(
            item.content.first(),
            Some(ContentPart::Reasoning { text, .. })
                if text == "Preparing friendly brief response"
        ));
    }
}
