import { writeTextFile } from '@tauri-apps/plugin-fs';
import { desktopDir, join } from '@tauri-apps/api/path';

let enabled = false;
let logLines: string[] = [];
let resolvedPath: string | null = null;
let flushTimer: ReturnType<typeof setTimeout> | null = null;

function ts(): string {
  return new Date().toISOString().replace('T', ' ').slice(0, 23);
}

async function getPath(): Promise<string> {
  if (!resolvedPath) {
    const desktop = await desktopDir();
    resolvedPath = await join(desktop, 'MAS-debug.log');
  }
  return resolvedPath;
}

async function flushNow(): Promise<void> {
  if (logLines.length === 0) return;
  try {
    const path = await getPath();
    await writeTextFile(path, logLines.join('\n') + '\n');
  } catch (e) {
    console.error('[DebugLogger] Write failed:', e);
  }
}

function scheduleFlush(): void {
  if (flushTimer) return;
  flushTimer = setTimeout(() => {
    flushTimer = null;
    flushNow();
  }, 80);
}

export function setDebugLoggingEnabled(value: boolean): void {
  if (value === enabled) return;
  enabled = value;
  if (value) {
    logLines = [];
    logLines.push('='.repeat(64));
    logLines.push(`MAS Debug Log — ${new Date().toLocaleString()}`);
    logLines.push('='.repeat(64));
    logLines.push('');
    scheduleFlush();
  }
}

export function isDebugEnabled(): boolean {
  return enabled;
}

export function debugLog(category: string, message: string, data?: unknown): void {
  if (!enabled) return;

  let line = `[${ts()}] [${category.toUpperCase().slice(0, 12).padEnd(12)}] ${message}`;
  if (data !== undefined) {
    try {
      const serialized = typeof data === 'string'
        ? data
        : JSON.stringify(data, null, 2);
      // indent multi-line data
      const indented = serialized.split('\n').join('\n    ');
      line += '\n    ' + indented;
    } catch {
      line += '\n    [unserializable data]';
    }
  }

  logLines.push(line);
  scheduleFlush();
}

/** Force-flush immediately (call before app exits or on demand). */
export async function flushDebugLog(): Promise<void> {
  if (flushTimer) {
    clearTimeout(flushTimer);
    flushTimer = null;
  }
  await flushNow();
}
