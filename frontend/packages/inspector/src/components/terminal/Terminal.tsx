import { useEffect, useRef, useState, useCallback } from "react";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import "@xterm/xterm/css/xterm.css";

export interface TerminalProps {
  /** WebSocket URL for terminal connection */
  wsUrl: string;
  /** Whether the terminal is currently active/focused */
  active?: boolean;
  /** Callback when the terminal is closed */
  onClose?: () => void;
  /** Callback when the terminal connection status changes */
  onConnectionChange?: (connected: boolean) => void;
  /** Initial number of columns */
  cols?: number;
  /** Initial number of rows */
  rows?: number;
}

interface TerminalMessage {
  type: "data" | "input" | "resize" | "exit" | "error";
  data?: string;
  cols?: number;
  rows?: number;
  code?: number | null;
  message?: string;
}

const Terminal = ({
  wsUrl,
  active = true,
  onClose,
  onConnectionChange,
  cols = 80,
  rows = 24,
}: TerminalProps) => {
  const terminalRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<XTerm | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Initialize terminal
  useEffect(() => {
    if (!terminalRef.current) return;

    const term = new XTerm({
      cursorBlink: true,
      fontSize: 13,
      fontFamily: '"JetBrains Mono", "Fira Code", "Cascadia Code", Menlo, Monaco, "Courier New", monospace',
      theme: {
        background: "#1a1a1a",
        foreground: "#d4d4d4",
        cursor: "#d4d4d4",
        cursorAccent: "#1a1a1a",
        selectionBackground: "#264f78",
        black: "#000000",
        red: "#cd3131",
        green: "#0dbc79",
        yellow: "#e5e510",
        blue: "#2472c8",
        magenta: "#bc3fbc",
        cyan: "#11a8cd",
        white: "#e5e5e5",
        brightBlack: "#666666",
        brightRed: "#f14c4c",
        brightGreen: "#23d18b",
        brightYellow: "#f5f543",
        brightBlue: "#3b8eea",
        brightMagenta: "#d670d6",
        brightCyan: "#29b8db",
        brightWhite: "#e5e5e5",
      },
      cols,
      rows,
    });

    const fitAddon = new FitAddon();
    const webLinksAddon = new WebLinksAddon();

    term.loadAddon(fitAddon);
    term.loadAddon(webLinksAddon);
    term.open(terminalRef.current);

    // Fit terminal to container
    setTimeout(() => fitAddon.fit(), 0);

    xtermRef.current = term;
    fitAddonRef.current = fitAddon;

    // Handle window resize
    const handleResize = () => {
      if (fitAddonRef.current && xtermRef.current) {
        fitAddonRef.current.fit();
        // Send resize to server
        const { cols, rows } = xtermRef.current;
        sendResize(cols, rows);
      }
    };

    window.addEventListener("resize", handleResize);

    return () => {
      window.removeEventListener("resize", handleResize);
      term.dispose();
      xtermRef.current = null;
      fitAddonRef.current = null;
    };
  }, [cols, rows]);

  // Send resize message
  const sendResize = useCallback((cols: number, rows: number) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      const msg: TerminalMessage = { type: "resize", cols, rows };
      wsRef.current.send(JSON.stringify(msg));
    }
  }, []);

  // Connect WebSocket
  useEffect(() => {
    if (!wsUrl || !xtermRef.current) return;

    setError(null);
    const ws = new WebSocket(wsUrl);
    wsRef.current = ws;

    ws.onopen = () => {
      setConnected(true);
      onConnectionChange?.(true);
      xtermRef.current?.writeln("\x1b[32m● Connected to terminal\x1b[0m\r\n");
      
      // Send initial resize
      if (fitAddonRef.current && xtermRef.current) {
        fitAddonRef.current.fit();
        const { cols, rows } = xtermRef.current;
        sendResize(cols, rows);
      }
    };

    ws.onmessage = (event) => {
      try {
        const msg: TerminalMessage = JSON.parse(event.data);
        
        switch (msg.type) {
          case "data":
            if (msg.data) {
              xtermRef.current?.write(msg.data);
            }
            break;
          case "exit":
            xtermRef.current?.writeln(`\r\n\x1b[33m● Process exited with code ${msg.code ?? "unknown"}\x1b[0m`);
            onClose?.();
            break;
          case "error":
            setError(msg.message || "Unknown error");
            xtermRef.current?.writeln(`\r\n\x1b[31m● Error: ${msg.message}\x1b[0m`);
            break;
        }
      } catch (e) {
        // Handle binary data
        if (event.data instanceof Blob) {
          event.data.text().then((text: string) => {
            xtermRef.current?.write(text);
          });
        }
      }
    };

    ws.onerror = () => {
      setError("WebSocket connection error");
      setConnected(false);
      onConnectionChange?.(false);
    };

    ws.onclose = () => {
      setConnected(false);
      onConnectionChange?.(false);
      xtermRef.current?.writeln("\r\n\x1b[31m● Disconnected from terminal\x1b[0m");
    };

    // Handle terminal input
    const onData = xtermRef.current.onData((data) => {
      if (ws.readyState === WebSocket.OPEN) {
        const msg: TerminalMessage = { type: "input", data };
        ws.send(JSON.stringify(msg));
      }
    });

    return () => {
      onData.dispose();
      ws.close();
      wsRef.current = null;
    };
  }, [wsUrl, onClose, onConnectionChange, sendResize]);

  // Handle container resize with ResizeObserver
  useEffect(() => {
    if (!terminalRef.current) return;

    const resizeObserver = new ResizeObserver(() => {
      if (fitAddonRef.current && xtermRef.current) {
        fitAddonRef.current.fit();
        const { cols, rows } = xtermRef.current;
        sendResize(cols, rows);
      }
    });

    resizeObserver.observe(terminalRef.current);

    return () => {
      resizeObserver.disconnect();
    };
  }, [sendResize]);

  // Focus terminal when active
  useEffect(() => {
    if (active && xtermRef.current) {
      xtermRef.current.focus();
    }
  }, [active]);

  return (
    <div className="terminal-container" style={{ height: "100%", position: "relative" }}>
      {error && (
        <div
          style={{
            position: "absolute",
            top: 8,
            right: 8,
            background: "var(--color-error)",
            color: "white",
            padding: "4px 8px",
            borderRadius: 4,
            fontSize: 12,
            zIndex: 10,
          }}
        >
          {error}
        </div>
      )}
      <div
        ref={terminalRef}
        style={{
          height: "100%",
          width: "100%",
          background: "#1a1a1a",
          borderRadius: 4,
          overflow: "hidden",
        }}
      />
      <div
        style={{
          position: "absolute",
          bottom: 4,
          right: 8,
          fontSize: 10,
          color: connected ? "var(--color-success)" : "var(--color-muted)",
        }}
      >
        {connected ? "● Connected" : "○ Disconnected"}
      </div>
    </div>
  );
};

export default Terminal;
