import {
  Activity,
  AlertTriangle,
  Brain,
  CheckCircle,
  FileDiff,
  HelpCircle,
  MessageSquare,
  PauseCircle,
  PlayCircle,
  Shield,
  Terminal,
  Wrench,
  Zap
} from "lucide-react";
import type { UniversalEvent } from "sandbox-agent";

export const getEventType = (event: UniversalEvent) => event.type;

export const getEventKey = (event: UniversalEvent) =>
  event.event_id ? `id:${event.event_id}` : `seq:${event.sequence}`;

export const getEventCategory = (type: string) => type.split(".")[0] ?? type;

export const getEventClass = (type: string) => type.replace(/\./g, "-");

export const getEventIcon = (type: string) => {
  switch (type) {
    case "session.started":
      return PlayCircle;
    case "session.ended":
      return PauseCircle;
    case "turn.started":
      return PlayCircle;
    case "turn.ended":
      return PauseCircle;
    case "item.started":
      return MessageSquare;
    case "item.delta":
      return Activity;
    case "item.completed":
      return CheckCircle;
    case "question.requested":
      return HelpCircle;
    case "question.resolved":
      return CheckCircle;
    case "permission.requested":
      return Shield;
    case "permission.resolved":
      return CheckCircle;
    case "error":
      return AlertTriangle;
    case "agent.unparsed":
      return Brain;
    default:
      if (type.startsWith("item.")) return MessageSquare;
      if (type.startsWith("session.")) return PlayCircle;
      if (type.startsWith("error")) return AlertTriangle;
      if (type.startsWith("agent.")) return Brain;
      if (type.startsWith("question.")) return HelpCircle;
      if (type.startsWith("permission.")) return Shield;
      if (type.startsWith("file.")) return FileDiff;
      if (type.startsWith("command.")) return Terminal;
      if (type.startsWith("tool.")) return Wrench;
      return Zap;
  }
};
