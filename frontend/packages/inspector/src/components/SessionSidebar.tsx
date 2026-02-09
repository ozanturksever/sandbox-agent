import { Plus, RefreshCw } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { AgentInfo, AgentModelInfo, AgentModeInfo, SessionInfo, SkillSource } from "sandbox-agent";
import type { McpServerEntry } from "../App";
import SessionCreateMenu, { type SessionConfig } from "./SessionCreateMenu";

const agentLabels: Record<string, string> = {
  claude: "Claude Code",
  codex: "Codex",
  opencode: "OpenCode",
  amp: "Amp",
  codebuff: "Codebuff",
  mock: "Mock"
};

const SessionSidebar = ({
  sessions,
  selectedSessionId,
  onSelectSession,
  onRefresh,
  onCreateSession,
  onSelectAgent,
  agents,
  agentsLoading,
  agentsError,
  sessionsLoading,
  sessionsError,
  modesByAgent,
  modelsByAgent,
  defaultModelByAgent,
  modesLoadingByAgent,
  modelsLoadingByAgent,
  modesErrorByAgent,
  modelsErrorByAgent,
  mcpServers,
  onMcpServersChange,
  mcpConfigError,
  skillSources,
  onSkillSourcesChange
}: {
  sessions: SessionInfo[];
  selectedSessionId: string;
  onSelectSession: (session: SessionInfo) => void;
  onRefresh: () => void;
  onCreateSession: (agentId: string, config: SessionConfig) => void;
  onSelectAgent: (agentId: string) => void;
  agents: AgentInfo[];
  agentsLoading: boolean;
  agentsError: string | null;
  sessionsLoading: boolean;
  sessionsError: string | null;
  modesByAgent: Record<string, AgentModeInfo[]>;
  modelsByAgent: Record<string, AgentModelInfo[]>;
  defaultModelByAgent: Record<string, string>;
  modesLoadingByAgent: Record<string, boolean>;
  modelsLoadingByAgent: Record<string, boolean>;
  modesErrorByAgent: Record<string, string | null>;
  modelsErrorByAgent: Record<string, string | null>;
  mcpServers: McpServerEntry[];
  onMcpServersChange: (servers: McpServerEntry[]) => void;
  mcpConfigError: string | null;
  skillSources: SkillSource[];
  onSkillSourcesChange: (sources: SkillSource[]) => void;
}) => {
  const [showMenu, setShowMenu] = useState(false);
  const menuRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!showMenu) return;
    const handler = (event: MouseEvent) => {
      if (!menuRef.current) return;
      if (!menuRef.current.contains(event.target as Node)) {
        setShowMenu(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [showMenu]);


  return (
    <div className="session-sidebar">
      <div className="sidebar-header">
        <span className="sidebar-title">Sessions</span>
        <div className="sidebar-header-actions">
          <button className="sidebar-icon-btn" onClick={onRefresh} title="Refresh sessions">
            <RefreshCw size={14} />
          </button>
          <div className="sidebar-add-menu-wrapper" ref={menuRef}>
            <button
              className="sidebar-add-btn"
              onClick={() => setShowMenu((value) => !value)}
              title="New session"
            >
              <Plus size={14} />
            </button>
            <SessionCreateMenu
              agents={agents}
              agentsLoading={agentsLoading}
              agentsError={agentsError}
              modesByAgent={modesByAgent}
              modelsByAgent={modelsByAgent}
              defaultModelByAgent={defaultModelByAgent}
              modesLoadingByAgent={modesLoadingByAgent}
              modelsLoadingByAgent={modelsLoadingByAgent}
              modesErrorByAgent={modesErrorByAgent}
              modelsErrorByAgent={modelsErrorByAgent}
              mcpServers={mcpServers}
              onMcpServersChange={onMcpServersChange}
              mcpConfigError={mcpConfigError}
              skillSources={skillSources}
              onSkillSourcesChange={onSkillSourcesChange}
              onSelectAgent={onSelectAgent}
              onCreateSession={onCreateSession}
              open={showMenu}
              onClose={() => setShowMenu(false)}
            />
          </div>
        </div>
      </div>

      <div className="session-list">
        {sessionsLoading ? (
          <div className="sidebar-empty">Loading sessions...</div>
        ) : sessionsError ? (
          <div className="sidebar-empty error">{sessionsError}</div>
        ) : sessions.length === 0 ? (
          <div className="sidebar-empty">No sessions yet.</div>
        ) : (
          sessions.map((session) => (
            <button
              key={session.sessionId}
              className={`session-item ${session.sessionId === selectedSessionId ? "active" : ""}`}
              onClick={() => onSelectSession(session)}
            >
              <div className="session-item-id">{session.sessionId}</div>
              <div className="session-item-meta">
                <span className="session-item-agent">{agentLabels[session.agent] ?? session.agent}</span>
                <span className="session-item-events">{session.eventCount} events</span>
                {session.ended && <span className="session-item-ended">ended</span>}
              </div>
            </button>
          ))
        )}
      </div>
    </div>
  );
};

export default SessionSidebar;
