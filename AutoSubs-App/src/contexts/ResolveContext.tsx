import React, { createContext, useContext, useState, useRef, useEffect } from 'react';
import { TimelineInfo } from '@/types/interfaces';
import { getTimelineInfo, cancelExport, addSubtitlesToTimeline } from '@/api/resolve-api';
import { debugLog } from '@/utils/debug-logger';

interface ResolveContextType {
  timelineInfo: TimelineInfo;
  markIn: number;
  isExporting: boolean;
  exportProgress: number;
  cancelRequestedRef: React.MutableRefObject<boolean>;
  lastConnectionError: string | null;
  connectionAttempts: number;
  refresh: () => Promise<void>;
  pushToTimeline: (filename?: string, selectedTemplate?: string, selectedOutputTrack?: string) => Promise<void>;
  getSourceAudio: (isStandaloneMode: boolean, fileInput: string | null, inputTracks: string[]) => Promise<{ path: string, offset: number } | null>;
  setIsExporting: (isExporting: boolean) => void;
  setExportProgress: (progress: number) => void;
  cancelExport: () => Promise<any>;
}

const ResolveContext = createContext<ResolveContextType | null>(null);

export function ResolveProvider({ children }: { children: React.ReactNode }) {
  const [timelineInfo, setTimelineInfo] = useState<TimelineInfo>({ name: "", timelineId: "", templates: [], inputTracks: [], outputTracks: [] });
  const [markIn] = useState(0);
  
  // Export state
  const [isExporting, setIsExporting] = useState<boolean>(false);
  const [exportProgress, setExportProgress] = useState<number>(0);
  const cancelRequestedRef = useRef<boolean>(false);

  // Connection diagnostics
  const [lastConnectionError, setLastConnectionError] = useState<string | null>(null);
  const [connectionAttempts, setConnectionAttempts] = useState<number>(0);

  // Initialize timeline info
  useEffect(() => {
    async function initializeTimeline() {
      const ts = new Date().toISOString();
      console.log(`[MAS ${ts}] Fetching timeline info from port 56003...`);
      debugLog('resolve', 'Initial connection attempt');
      setConnectionAttempts(prev => prev + 1);
      try {
        const info = await getTimelineInfo().catch((err) => {
          const msg = err instanceof Error ? err.message : String(err);
          console.log(`[MAS ${ts}] Resolve offline: ${msg}`);
          debugLog('resolve', 'Initial connection failed', msg);
          setLastConnectionError(`${ts}: ${msg}`);
          return null;
        });

        if (info && info.timelineId) {
          console.log(`[MAS ${ts}] Connected — timeline "${info.name}" (id=${info.timelineId})`);
          debugLog('resolve', 'Connected', { timeline: info.name, id: info.timelineId });
          setLastConnectionError(null);
          setTimelineInfo(info);
        } else if (info) {
          const msg = `Connected but no active timeline (timelineId empty). Response: ${JSON.stringify(info)}`;
          console.log(`[MAS ${ts}] ${msg}`);
          debugLog('resolve', 'No active timeline', info);
          setLastConnectionError(`${ts}: ${msg}`);
        }
      } catch (error) {
        const msg = error instanceof Error ? error.message : String(error);
        console.error(`[MAS ${ts}] Error initializing timeline: ${msg}`);
        debugLog('resolve', 'Init error', msg);
        setLastConnectionError(`${ts}: ${msg}`);
      }
    }

    initializeTimeline();
  }, []);

  // Poll for Resolve connection every 5 seconds when not connected
  useEffect(() => {
    if (timelineInfo.timelineId) return;

    const interval = setInterval(async () => {
      const ts = new Date().toISOString();
      setConnectionAttempts(prev => prev + 1);
      try {
        const info = await getTimelineInfo().catch((err) => {
          const msg = err instanceof Error ? err.message : String(err);
          setLastConnectionError(`${ts}: ${msg}`);
          return null;
        });
        if (info?.timelineId) {
          console.log(`[MAS ${ts}] Poll connected — timeline "${info.name}"`);
          setLastConnectionError(null);
          setTimelineInfo(info);
        }
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        setLastConnectionError(`${ts}: ${msg}`);
      }
    }, 5000);

    return () => clearInterval(interval);
  }, [timelineInfo.timelineId]);

  async function refresh() {
    try {
      let newTimelineInfo = await getTimelineInfo();
      setTimelineInfo(newTimelineInfo);
    } catch (error) {
      // setError will be handled by calling context if needed
      console.error("Failed to get current timeline:", error);
      throw error;
    }
  }

  async function pushToTimeline(filename?: string, selectedTemplate?: string, selectedOutputTrack?: string) {
    const finalFilename = filename || '';
    const finalTemplate = selectedTemplate || 'Subtitle';
    const finalTrack = selectedOutputTrack || '1';
    debugLog('resolve', 'pushToTimeline →', { filename: finalFilename, template: finalTemplate, track: finalTrack });
    try {
      await addSubtitlesToTimeline(finalFilename, finalTemplate, finalTrack);
      debugLog('resolve', 'pushToTimeline ← OK');
    } catch (err) {
      debugLog('resolve', 'pushToTimeline ← ERROR', String(err));
      throw err;
    }
  }

  // Function to get source audio based on current mode
  const getSourceAudio = async (
    isStandaloneMode: boolean,
    fileInput: string | null,
    inputTracks: string[]
  ): Promise<{ path: string, offset: number } | null> => {
    if (timelineInfo && !isStandaloneMode) {
      // Reset cancellation flag at the start of export
      cancelRequestedRef.current = false;
      setIsExporting(true);
      setExportProgress(0);

      try {
        // Import the required functions directly
        const { exportAudio, getExportProgress } = await import('@/api/resolve-api');

        debugLog('resolve', 'ExportAudio starting', { inputTracks });
        const exportResult = await exportAudio(inputTracks);
        console.log("Export started:", exportResult);
        debugLog('resolve', 'ExportAudio started', exportResult);

        // Poll for export progress until completion
        let exportCompleted = false;
        let audioInfo = null;

        while (!exportCompleted && !cancelRequestedRef.current) {
          // Check if cancellation was requested before making the next API call
          if (cancelRequestedRef.current) {
            console.log("Export polling interrupted by cancellation request");
            break;
          }

          const progressResult = await getExportProgress();
          console.log("Export progress:", progressResult);

          // Update progress
          setExportProgress(progressResult.progress || 0);

          if (progressResult.completed) {
            exportCompleted = true;
            audioInfo = progressResult.audioInfo;
            console.log("Export completed:", audioInfo);
            debugLog('resolve', 'Audio export complete', audioInfo);
          } else if (progressResult.cancelled) {
            console.log("Export was cancelled");
            setIsExporting(false);
            setExportProgress(0);
            return null;
          } else if (progressResult.error) {
            console.error("Export error:", progressResult.message);
            debugLog('resolve', 'Audio export ERROR', progressResult.message);
            setIsExporting(false);
            setExportProgress(0);
            throw new Error(progressResult.message || "Export failed");
          }

          // Wait before next poll (avoid overwhelming the server)
          if (!exportCompleted && !cancelRequestedRef.current) {
            await new Promise(resolve => setTimeout(resolve, 500));

            // Check again after timeout in case cancellation happened during the wait
            if (cancelRequestedRef.current) {
              console.log("Export polling interrupted during wait interval");
              break;
            }
          }
        }

        setIsExporting(false);
        setExportProgress(100);

        let audioPath = audioInfo["path"];
        let audioOffset = audioInfo["offset"];
        return { path: audioPath, offset: audioOffset };

      } catch (error) {
        setIsExporting(false);
        setExportProgress(0);
        throw error;
      }
    } else {
      return { path: fileInput || "", offset: 0 };
    }
  };

  return (
    <ResolveContext.Provider value={{
      timelineInfo,
      markIn,
      isExporting,
      exportProgress,
      cancelRequestedRef,
      lastConnectionError,
      connectionAttempts,
      refresh,
      pushToTimeline,
      getSourceAudio,
      setIsExporting,
      setExportProgress,
      cancelExport,
    }}>
      {children}
    </ResolveContext.Provider>
  );
}

export const useResolve = () => {
  const context = useContext(ResolveContext);
  if (!context) {
    throw new Error('useResolve must be used within a ResolveProvider');
  }
  return context;
};
