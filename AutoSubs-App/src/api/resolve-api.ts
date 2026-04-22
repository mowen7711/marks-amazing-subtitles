import { fetch } from '@tauri-apps/plugin-http';
import { downloadDir } from '@tauri-apps/api/path';
import { getTranscriptPath } from '@/utils/file-utils';
import { Speaker } from '@/types/interfaces';
import { debugLog } from '@/utils/debug-logger';

const resolveAPI = "http://localhost:56003/";

export async function exportAudio(inputTracks: Array<string>) {
  const outputDir = await downloadDir();
  debugLog('resolve', 'ExportAudio →', { inputTracks, outputDir });
  const response = await fetch(resolveAPI, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      func: "ExportAudio",
      outputDir,
      inputTracks,
    }),
  });
  const data = await response.json();
  debugLog('resolve', 'ExportAudio ←', data);

  if (data.error) {
    throw new Error(data.message || "Failed to start audio export");
  }
  if (!data.started) {
    throw new Error("Export did not start successfully");
  }
  return data;
}

export async function jumpToTime(seconds: number) {
  const response = await fetch(resolveAPI, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ func: "JumpToTime", seconds }),
  });
  return response.json();
}

export async function getTimelineInfo() {
  debugLog('resolve', 'GetTimelineInfo →');
  const response = await fetch(resolveAPI, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ func: "GetTimelineInfo" }),
  });
  const data = await response.json();
  if (!data.timelineId) {
    debugLog('resolve', 'GetTimelineInfo ← no timeline', data);
    throw new Error("No timeline detected in Resolve.");
  }
  debugLog('resolve', 'GetTimelineInfo ← OK', { timelineId: data.timelineId, name: data.name });
  return data;
}

export interface ConflictInfo {
  hasConflicts: boolean;
  conflictingClips?: Array<{ start: number; end: number; name: string }>;
  trackName?: string;
  subtitleRange?: { start: number; end: number };
  totalConflicts?: number;
  trackExists?: boolean;
  message?: string;
  error?: string;
}

export type ConflictMode = 'replace' | 'skip' | 'new_track' | null;

export async function checkTrackConflicts(filename: string, outputTrack: string): Promise<ConflictInfo> {
  const filePath = await getTranscriptPath(filename);
  debugLog('resolve', 'CheckTrackConflicts →', { filePath, outputTrack });
  const response = await fetch(resolveAPI, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      func: "CheckTrackConflicts",
      filePath,
      trackIndex: outputTrack,
    }),
  });
  return response.json();
}

export async function addSubtitlesToTimeline(
  filename: string,
  currentTemplate: string,
  outputTrack: string,
  conflictMode: ConflictMode = null
) {
  const filePath = await getTranscriptPath(filename);
  debugLog('resolve', 'AddSubtitles →', { filePath, templateName: currentTemplate, trackIndex: outputTrack, conflictMode });
  const response = await fetch(resolveAPI, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      func: "AddSubtitles",
      filePath,
      templateName: currentTemplate,
      trackIndex: outputTrack,
      conflictMode,
    }),
  });
  const data = await response.json();
  debugLog('resolve', 'AddSubtitles ←', data);
  if (typeof data.message === 'string' && data.message.startsWith('Job failed')) {
    throw new Error(data.message);
  }
  if (data.result === false) {
    throw new Error('Failed to add subtitles. Check that the subtitle template exists in your DaVinci Resolve media pool.');
  }
  return data;
}

export async function closeResolveLink() {
  const response = await fetch(resolveAPI, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ func: "Exit" }),
  });
  const result = await response.json();
  debugLog('resolve', 'CheckTrackConflicts ←', result);
  return result;
}

export async function getExportProgress() {
  const response = await fetch(resolveAPI, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ func: "GetExportProgress" }),
  });
  return response.json();
}

export async function cancelExport() {
  const response = await fetch(resolveAPI, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ func: "CancelExport" }),
  });
  return response.json();
}

export async function getRenderJobStatus() {
  const response = await fetch(resolveAPI, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ func: "GetRenderJobStatus" }),
  });
  return response.json();
}

export async function generatePreview(speaker: Speaker, templateName: string, exportPath: string) {
  const response = await fetch(resolveAPI, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ func: "GeneratePreview", speaker, templateName, exportPath }),
  });
  return response.json();
}
