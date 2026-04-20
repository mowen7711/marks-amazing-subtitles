import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { RefreshCw, Copy, CheckCheck } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { useResolve } from "@/contexts/ResolveContext";

interface AppDiagnostics {
  app_version: string;
  log_dir: string;
  platform: string;
  arch: string;
}

interface DiagnosticsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function DiagnosticsDialog({ open, onOpenChange }: DiagnosticsDialogProps) {
  const { timelineInfo, lastConnectionError, connectionAttempts } = useResolve();
  const [luaLog, setLuaLog] = useState<string>("");
  const [backendLog, setBackendLog] = useState<string>("");
  const [diagnostics, setDiagnostics] = useState<AppDiagnostics | null>(null);
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState(false);

  const load = async () => {
    setLoading(true);
    try {
      const [lua, backend, diag] = await Promise.all([
        invoke<string>("get_lua_log"),
        invoke<string>("get_backend_logs"),
        invoke<AppDiagnostics>("get_app_diagnostics"),
      ]);
      setLuaLog(lua);
      // Keep only last 100 lines of backend log to keep the UI readable
      const lines = backend.split("\n");
      setBackendLog(lines.slice(-100).join("\n"));
      setDiagnostics(diag);
    } catch (e) {
      console.error("Failed to load diagnostics:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (open) load();
  }, [open]);

  const copyAll = async () => {
    const text = [
      "=== App Diagnostics ===",
      diagnostics ? JSON.stringify(diagnostics, null, 2) : "(not loaded)",
      "",
      "=== Resolve Connection ===",
      `Connected: ${!!timelineInfo?.timelineId}`,
      `Timeline: ${timelineInfo?.name || "(none)"}`,
      `Timeline ID: ${timelineInfo?.timelineId || "(none)"}`,
      `Connection attempts: ${connectionAttempts}`,
      `Last error: ${lastConnectionError || "(none)"}`,
      "",
      "=== Lua Launch Log ===",
      luaLog,
      "",
      "=== Backend Log (last 100 lines) ===",
      backendLog,
    ].join("\n");
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>Diagnostics</DialogTitle>
          <DialogDescription>
            App and connection diagnostics for troubleshooting.
          </DialogDescription>
        </DialogHeader>

        <div className="flex gap-2 shrink-0">
          <Button variant="outline" size="sm" onClick={load} disabled={loading}>
            <RefreshCw className={`h-3.5 w-3.5 mr-1.5 ${loading ? "animate-spin" : ""}`} />
            Refresh
          </Button>
          <Button variant="outline" size="sm" onClick={copyAll}>
            {copied ? <CheckCheck className="h-3.5 w-3.5 mr-1.5 text-green-500" /> : <Copy className="h-3.5 w-3.5 mr-1.5" />}
            Copy All
          </Button>
        </div>

        <div className="overflow-y-auto flex-1 space-y-4 text-xs font-mono">
          {/* App Info */}
          <section>
            <h3 className="font-semibold text-sm font-sans mb-1">App Info</h3>
            <div className="bg-muted rounded p-2 space-y-0.5">
              <div>Version: {diagnostics?.app_version ?? "…"}</div>
              <div>Platform: {diagnostics?.platform ?? "…"} / {diagnostics?.arch ?? "…"}</div>
              <div className="break-all">Log dir: {diagnostics?.log_dir ?? "…"}</div>
            </div>
          </section>

          {/* Resolve Connection */}
          <section>
            <h3 className="font-semibold text-sm font-sans mb-1">Resolve Connection (port 56003)</h3>
            <div className="bg-muted rounded p-2 space-y-0.5">
              <div>
                Status:{" "}
                <span className={timelineInfo?.timelineId ? "text-green-500" : "text-red-500"}>
                  {timelineInfo?.timelineId ? "Connected" : "Disconnected"}
                </span>
              </div>
              {timelineInfo?.timelineId && (
                <>
                  <div>Timeline: {timelineInfo.name || "(unnamed)"}</div>
                  <div>Timeline ID: {timelineInfo.timelineId}</div>
                </>
              )}
              <div>Connection attempts: {connectionAttempts}</div>
              {lastConnectionError && (
                <div className="text-red-400 break-all">Last error: {lastConnectionError}</div>
              )}
            </div>
          </section>

          {/* Lua Log */}
          <section>
            <h3 className="font-semibold text-sm font-sans mb-1">
              Lua Launch Log <span className="font-normal text-muted-foreground">(%TEMP%\MarksAmazingSubs_launch.log)</span>
            </h3>
            <pre className="bg-muted rounded p-2 whitespace-pre-wrap break-all max-h-48 overflow-y-auto text-[11px]">
              {luaLog || "(empty — run 'Marks Amazing Subtitles' from DaVinci Resolve Scripts menu first)"}
            </pre>
          </section>

          {/* Backend Log */}
          <section>
            <h3 className="font-semibold text-sm font-sans mb-1">Backend Log (last 100 lines)</h3>
            <pre className="bg-muted rounded p-2 whitespace-pre-wrap break-all max-h-64 overflow-y-auto text-[11px]">
              {backendLog || "(empty)"}
            </pre>
          </section>
        </div>
      </DialogContent>
    </Dialog>
  );
}
