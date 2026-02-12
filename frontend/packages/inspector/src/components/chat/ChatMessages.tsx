import { useState } from "react";
import { getAvatarLabel, getMessageClass } from "./messageUtils";
import type { TimelineEntry } from "./types";
import { AlertTriangle, Settings, ChevronRight, ChevronDown } from "lucide-react";

const CollapsibleMessage = ({
  id,
  icon,
  label,
  children,
  className = ""
}: {
  id: string;
  icon: React.ReactNode;
  label: string;
  children: React.ReactNode;
  className?: string;
}) => {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className={`collapsible-message ${className}`}>
      <button className="collapsible-header" onClick={() => setExpanded(!expanded)}>
        {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        {icon}
        <span>{label}</span>
      </button>
      {expanded && <div className="collapsible-content">{children}</div>}
    </div>
  );
};

const ChatMessages = ({
  entries,
  sessionError,
  messagesEndRef
}: {
  entries: TimelineEntry[];
  sessionError: string | null;
  messagesEndRef: React.RefObject<HTMLDivElement>;
}) => {
  return (
    <div className="messages">
      {entries.map((entry) => {
        const messageClass = getMessageClass(entry);

        if (entry.kind === "meta") {
          const isError = entry.meta?.severity === "error";
          const title = entry.meta?.title ?? "Status";
          const isStatusDivider = ["Session Started", "Turn Started", "Turn Ended"].includes(title);

          if (isStatusDivider) {
            return (
              <div key={entry.id} className="status-divider">
                <div className="status-divider-line" />
                <span className="status-divider-text">
                  <Settings size={12} />
                  {title}
                </span>
                <div className="status-divider-line" />
              </div>
            );
          }

          return (
            <CollapsibleMessage
              key={entry.id}
              id={entry.id}
              icon={isError ? <AlertTriangle size={14} className="error-icon" /> : <Settings size={14} className="system-icon" />}
              label={title}
              className={isError ? "error" : "system"}
            >
              {entry.meta?.detail && <div className="part-body">{entry.meta.detail}</div>}
            </CollapsibleMessage>
          );
        }

        if (entry.kind === "reasoning") {
          return (
            <div key={entry.id} className="message assistant">
              <div className="avatar">{getAvatarLabel("assistant")}</div>
              <div className="message-content">
                <div className="message-meta">
                  <span>reasoning - {entry.reasoning?.visibility ?? "public"}</span>
                </div>
                <div className="part-body muted">{entry.reasoning?.text ?? ""}</div>
              </div>
            </div>
          );
        }

        if (entry.kind === "tool") {
          const isComplete = entry.toolStatus === "completed" || entry.toolStatus === "failed";
          const isFailed = entry.toolStatus === "failed";
          const statusLabel = entry.toolStatus && entry.toolStatus !== "completed"
            ? entry.toolStatus.replace("_", " ")
            : "";

          return (
            <CollapsibleMessage
              key={entry.id}
              id={entry.id}
              icon={<span className="tool-icon">T</span>}
              label={`tool call - ${entry.toolName ?? "tool"}${statusLabel ? ` (${statusLabel})` : ""}`}
              className={`tool${isFailed ? " error" : ""}`}
            >
              {entry.toolInput && <pre className="code-block">{entry.toolInput}</pre>}
              {isComplete && entry.toolOutput && (
                <div className="part">
                  <div className="part-title">result</div>
                  <pre className="code-block">{entry.toolOutput}</pre>
                </div>
              )}
              {!isComplete && !entry.toolInput && (
                <span className="thinking-indicator">
                  <span className="thinking-dot" />
                  <span className="thinking-dot" />
                  <span className="thinking-dot" />
                </span>
              )}
            </CollapsibleMessage>
          );
        }

        return (
          <div key={entry.id} className={`message ${messageClass}`}>
            <div className="avatar">{getAvatarLabel(messageClass)}</div>
            <div className="message-content">
              {entry.text ? (
                <div className="part-body">{entry.text}</div>
              ) : (
                <span className="thinking-indicator">
                  <span className="thinking-dot" />
                  <span className="thinking-dot" />
                  <span className="thinking-dot" />
                </span>
              )}
            </div>
          </div>
        );
      })}
      {sessionError && <div className="message-error">{sessionError}</div>}
      <div ref={messagesEndRef} />
    </div>
  );
};

export default ChatMessages;
