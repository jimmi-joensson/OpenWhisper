// TASK-91: Transcribe route. pick or drop an audio/video file, watch
// progress, copy the transcript.
//
// All job state (phase, progress, status line) comes from
// core::file_transcription via `useFileTranscription`; this component
// only renders the snapshot. Drag-drop uses the webview's native
// drop event because it carries real file paths (HTML5 drop doesn't).

import { useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { Check, Copy, FileAudio, X } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "./ui/alert";
import { Button } from "./ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "./ui/card";
import { Progress, ProgressLabel, ProgressValue } from "./ui/progress";
import { Spinner } from "./ui/spinner";
import { cn } from "@/lib/utils";
import {
  FILE_PHASE_CANCELLED,
  FILE_PHASE_DONE,
  FILE_PHASE_ERROR,
  FILE_PHASE_TRANSCRIBING,
  formatDuration,
  useFileTranscription,
} from "../lib/use-file-transcription";

export function TranscribePane() {
  const { snapshot, start, pick, cancel, startError } = useFileTranscription();
  const [dragOver, setDragOver] = useState(false);
  const [copied, setCopied] = useState(false);
  const busyRef = useRef(snapshot.busy);
  busyRef.current = snapshot.busy;

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        const p = event.payload;
        if (p.type === "enter" || p.type === "over") {
          setDragOver(!busyRef.current);
        } else if (p.type === "leave") {
          setDragOver(false);
        } else if (p.type === "drop") {
          setDragOver(false);
          const path = p.paths[0];
          if (path && !busyRef.current) void start(path);
        }
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [start]);

  // A new result resets the copy confirmation.
  useEffect(() => setCopied(false), [snapshot.text]);

  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(snapshot.text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch {
      // Clipboard may be unavailable in some contexts; UI stays as-is.
    }
  };

  const { phase, busy } = snapshot;
  const done = phase === FILE_PHASE_DONE;
  const hasText = done && snapshot.text.trim().length > 0;
  // Decoding and model load have no measurable progress; show the bar
  // as indeterminate until recognition starts.
  const progressValue =
    phase === FILE_PHASE_TRANSCRIBING ? Math.round(snapshot.progress * 100) : null;

  return (
    <div className="ow-transcribe" data-testid="transcribe-pane">
      <header className="ow-transcribe__header">
        <h1 className="ow-transcribe__title">Transcribe a file</h1>
        <p className="ow-transcribe__sub">
          Audio or video, any language Parakeet knows. Runs on this device; nothing is
          uploaded.
        </p>
      </header>

      <div
        className={cn("ow-transcribe__drop", dragOver && "ow-transcribe__drop--over")}
        data-testid="transcribe-dropzone"
        data-drag-over={dragOver || undefined}
        aria-disabled={busy || undefined}
      >
        <FileAudio className="ow-transcribe__drop-icon" aria-hidden="true" />
        <p className="ow-transcribe__drop-text">
          {dragOver ? "Drop to transcribe" : "Drop an audio or video file here"}
        </p>
        <Button
          variant="outline"
          data-testid="transcribe-choose"
          disabled={busy}
          onClick={() => void pick()}
        >
          Choose file…
        </Button>
        <p className="ow-transcribe__formats">
          MP3, M4A, WAV, OPUS, OGG, FLAC, MP4, MOV, WEBM, MKV
        </p>
      </div>

      {startError && (
        <Alert variant="destructive" data-testid="transcribe-start-error">
          <AlertTitle>Couldn't start</AlertTitle>
          <AlertDescription>{startError}</AlertDescription>
        </Alert>
      )}

      {busy && (
        <Card data-testid="transcribe-progress">
          <CardHeader>
            <CardTitle className="truncate">{snapshot.file_name}</CardTitle>
            {snapshot.audio_ms > 0 && (
              <CardDescription>{formatDuration(snapshot.audio_ms)} of audio</CardDescription>
            )}
            <CardAction>
              <Button
                variant="ghost"
                size="sm"
                data-testid="transcribe-cancel"
                disabled={snapshot.cancelling}
                onClick={() => void cancel()}
              >
                <X data-icon="inline-start" />
                Cancel
              </Button>
            </CardAction>
          </CardHeader>
          <CardContent>
            <Progress value={progressValue}>
              <ProgressLabel className="ow-transcribe__status" data-testid="transcribe-status">
                {progressValue === null && <Spinner />}
                {snapshot.status}
              </ProgressLabel>
              {progressValue !== null && <ProgressValue />}
            </Progress>
          </CardContent>
        </Card>
      )}

      {phase === FILE_PHASE_ERROR && (
        <Alert variant="destructive" data-testid="transcribe-error">
          <AlertTitle>{snapshot.file_name}</AlertTitle>
          <AlertDescription>{snapshot.status}</AlertDescription>
        </Alert>
      )}

      {phase === FILE_PHASE_CANCELLED && (
        <p className="ow-transcribe__note" data-testid="transcribe-cancelled">
          Cancelled {snapshot.file_name}.
        </p>
      )}

      {done && (
        <Card data-testid="transcribe-result">
          <CardHeader>
            <CardTitle className="truncate">{snapshot.file_name}</CardTitle>
            <CardDescription>
              {formatDuration(snapshot.audio_ms)} of audio · transcribed in{" "}
              {(snapshot.elapsed_ms / 1000).toFixed(1)} s
            </CardDescription>
            {hasText && (
              <CardAction>
                <Button
                  variant="outline"
                  size="sm"
                  data-testid="transcribe-copy"
                  onClick={() => void onCopy()}
                >
                  {copied ? <Check data-icon="inline-start" /> : <Copy data-icon="inline-start" />}
                  {copied ? "Copied" : "Copy"}
                </Button>
              </CardAction>
            )}
          </CardHeader>
          <CardContent>
            {hasText ? (
              <p className="ow-transcribe__text ow-selectable" data-testid="transcribe-text">
                {snapshot.text}
              </p>
            ) : (
              <p className="ow-transcribe__note">{snapshot.status}</p>
            )}
          </CardContent>
        </Card>
      )}
    </div>
  );
}
