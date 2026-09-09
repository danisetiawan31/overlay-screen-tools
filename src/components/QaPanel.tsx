import React, { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import { Mic, MicOff, AlertCircle, MessageSquare, Sparkles, Radio } from "lucide-react";
import { commands, events } from "../bindings";
import { markdownComponents } from "./MermaidRenderer";

export type QaStatus = "idle" | "recording" | "transcribing" | "answering" | "error";

export interface QaState {
  question: string | null;
  answer: string | null;
  status: QaStatus;
  error: string | null;
}

interface QaPanelProps {
  qaAvailable?: boolean;
}

export const QaPanel: React.FC<QaPanelProps> = ({ qaAvailable = true }) => {
  const [qaState, setQaState] = useState<QaState>({
    question: null,
    answer: null,
    status: "idle",
    error: null,
  });

  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const audioStreamRef = useRef<MediaStream | null>(null);
  const audioChunksRef = useRef<Blob[]>([]);
  const isRecordingRef = useRef<boolean>(false);

  // 1. Registrasi event listeners untuk alur Live Q&A
  useEffect(() => {
    let isCleanedUp = false;
    let unlistenStarted: (() => void) | null = null;
    let unlistenEnded: (() => void) | null = null;
    let unlistenTranscript: (() => void) | null = null;
    let unlistenAnswer: (() => void) | null = null;
    let unlistenError: (() => void) | null = null;

    // A. Listener qa:recording-started (F8 ditekan)
    listen("qa:recording-started", async () => {
      if (isCleanedUp || !qaAvailable) return;

      setQaState((prev) => ({
        ...prev,
        status: "recording",
        error: null,
      }));

      try {
        let stream = audioStreamRef.current;
        if (!stream || !stream.active) {
          stream = await navigator.mediaDevices.getUserMedia({ audio: true });
          audioStreamRef.current = stream;
        }

        audioChunksRef.current = [];
        const options: MediaRecorderOptions = MediaRecorder.isTypeSupported(
          "audio/webm;codecs=opus"
        )
          ? { mimeType: "audio/webm;codecs=opus" }
          : {};

        const recorder = new MediaRecorder(stream, options);
        mediaRecorderRef.current = recorder;
        isRecordingRef.current = true;

        recorder.ondataavailable = (e) => {
          if (e.data && e.data.size > 0) {
            audioChunksRef.current.push(e.data);
          }
        };

        recorder.start();
      } catch (err: unknown) {
        isRecordingRef.current = false;
        const errMsg =
          err instanceof Error
            ? err.message
            : "Akses mikrofon ditolak atau tidak tersedia.";
        setQaState((prev) => ({
          ...prev,
          status: "error",
          error: `Mikrofon: ${errMsg}. Pastikan izin mikrofon telah diberikan di Windows.`,
        }));
      }
    }).then((unsub) => {
      if (isCleanedUp) {
        unsub();
      } else {
        unlistenStarted = unsub;
      }
    });

    // B. Listener qa:recording-ended (F8 dilepas)
    events.qaRecordingEnded
      .listen(async (event) => {
        if (isCleanedUp) return;

        const { belowThreshold } = event.payload;
        const recorder = mediaRecorderRef.current;

        if (belowThreshold) {
          // Durasi tahan < 400ms -> buang audio diam-diam (no-op)
          if (recorder && isRecordingRef.current && recorder.state !== "inactive") {
            recorder.onstop = null;
            recorder.stop();
          }
          isRecordingRef.current = false;
          audioChunksRef.current = [];
          setQaState((prev) => ({
            ...prev,
            status: prev.status === "recording" ? "idle" : prev.status,
          }));
          return;
        }

        // Durasi tahan valid (>= 400ms)
        if (!recorder || !isRecordingRef.current || recorder.state === "inactive") {
          // Jika recorder tidak aktif (misal mic error saat start), jangan invoke
          return;
        }

        recorder.onstop = async () => {
          if (isCleanedUp) return;

          const audioBlob = new Blob(audioChunksRef.current, {
            type: "audio/webm;codecs=opus",
          });
          audioChunksRef.current = [];

          if (audioBlob.size === 0) {
            setQaState((prev) => ({ ...prev, status: "idle" }));
            return;
          }

          setQaState((prev) => ({
            ...prev,
            status: "transcribing",
            error: null,
          }));

          try {
            const arrayBuffer = await audioBlob.arrayBuffer();
            const bytes = Array.from(new Uint8Array(arrayBuffer));
            const res = await commands.sendAudioBlob({ bytes });

            if (res.status === "error") {
              setQaState((prev) => ({
                ...prev,
                status: "error",
                error: res.error,
              }));
            }
          } catch (err: unknown) {
            const msg = err instanceof Error ? err.message : String(err);
            setQaState((prev) => ({
              ...prev,
              status: "error",
              error: `Gagal mengirim audio: ${msg}`,
            }));
          }
        };

        isRecordingRef.current = false;
        recorder.stop();
      })
      .then((unsub) => {
        if (isCleanedUp) {
          unsub();
        } else {
          unlistenEnded = unsub;
        }
      });

    // C. Listener transcript:result (STT selesai)
    events.transcriptResult
      .listen((event) => {
        if (isCleanedUp) return;
        setQaState((prev) => ({
          ...prev,
          question: event.payload.text,
          status: "answering",
          error: null,
        }));
      })
      .then((unsub) => {
        if (isCleanedUp) {
          unsub();
        } else {
          unlistenTranscript = unsub;
        }
      });

    // D. Listener answer:result (AI selesai)
    events.answerResult
      .listen((event) => {
        if (isCleanedUp) return;
        setQaState((prev) => ({
          ...prev,
          answer: event.payload.text,
          status: "idle",
          error: null,
        }));
      })
      .then((unsub) => {
        if (isCleanedUp) {
          unsub();
        } else {
          unlistenAnswer = unsub;
        }
      });

    // E. Listener qa:error (Error STT / AI dari core)
    events.qaError
      .listen((event) => {
        if (isCleanedUp) return;
        setQaState((prev) => ({
          ...prev,
          status: "error",
          error: `[${event.payload.stage.toUpperCase()}] ${event.payload.message}`,
        }));
      })
      .then((unsub) => {
        if (isCleanedUp) {
          unsub();
        } else {
          unlistenError = unsub;
        }
      });

    return () => {
      isCleanedUp = true;
      if (unlistenStarted) unlistenStarted();
      if (unlistenEnded) unlistenEnded();
      if (unlistenTranscript) unlistenTranscript();
      if (unlistenAnswer) unlistenAnswer();
      if (unlistenError) unlistenError();

      if (mediaRecorderRef.current && mediaRecorderRef.current.state !== "inactive") {
        mediaRecorderRef.current.stop();
      }
      if (audioStreamRef.current) {
        audioStreamRef.current.getTracks().forEach((track) => track.stop());
      }
    };
  }, [qaAvailable]);

  if (!qaAvailable) {
    return (
      <div className="h-full flex flex-col items-center justify-center p-6 text-center text-zinc-500 bg-zinc-900/30 rounded-lg border border-zinc-800/60">
        <MicOff className="w-8 h-8 mb-2 text-zinc-600" />
        <h3 className="font-semibold text-zinc-400 text-sm mb-1">Live Q&A Tidak Tersedia</h3>
        <p className="text-xs max-w-sm">
          Hotkey F8 gagal didaftarkan pada sesi ini. Fitur push-to-talk dinonaktifkan secara otomatis.
        </p>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col space-y-3 overflow-hidden select-text">
      {/* Status Bar */}
      <div className="flex items-center justify-between px-3 py-2 bg-zinc-900/60 rounded-md border border-zinc-800/80 text-xs">
        <div className="flex items-center space-x-2">
          {qaState.status === "recording" && (
            <span className="flex items-center space-x-1.5 text-red-400 font-medium animate-pulse">
              <Radio className="w-4 h-4" />
              <span>Merekam suara... (Lepas F8 untuk selesai)</span>
            </span>
          )}
          {qaState.status === "transcribing" && (
            <span className="flex items-center space-x-1.5 text-amber-400 font-medium">
              <span className="w-2 h-2 rounded-full bg-amber-400 animate-ping" />
              <span>Mentranskripsi suara (Groq Whisper)...</span>
            </span>
          )}
          {qaState.status === "answering" && (
            <span className="flex items-center space-x-1.5 text-blue-400 font-medium">
              <Sparkles className="w-4 h-4 animate-spin" />
              <span>Menghasilkan contekan AI (OpenRouter)...</span>
            </span>
          )}
          {qaState.status === "idle" && (
            <span className="flex items-center space-x-1.5 text-zinc-400">
              <Mic className="w-3.5 h-3.5 text-emerald-400" />
              <span>Siap — Tahan <kbd className="px-1 py-0.5 bg-zinc-800 rounded border border-zinc-700 text-zinc-300 font-mono">F8</kbd> untuk bertanya</span>
            </span>
          )}
          {qaState.status === "error" && (
            <span className="flex items-center space-x-1.5 text-red-400 font-medium">
              <AlertCircle className="w-3.5 h-3.5" />
              <span>Terjadi kendala</span>
            </span>
          )}
        </div>
      </div>

      {/* Error Banner */}
      {qaState.error && (
        <div
          role="alert"
          className="bg-red-950/80 border border-red-800/80 px-3 py-2 rounded text-xs text-red-300 flex items-start justify-between"
        >
          <div className="flex items-start space-x-2">
            <AlertCircle className="w-4 h-4 text-red-400 shrink-0 mt-0.5" />
            <span>{qaState.error}</span>
          </div>
          <button
            type="button"
            onClick={() => setQaState((prev) => ({ ...prev, error: null }))}
            className="text-red-400 hover:text-red-200 ml-2 font-bold text-sm"
            aria-label="Tutup error"
          >
            ×
          </button>
        </div>
      )}

      {/* Main Q&A Content Area */}
      <div className="flex-1 flex flex-col space-y-3 overflow-hidden min-h-0">
        {/* Question Panel */}
        <div className="bg-zinc-900/40 border border-zinc-800/60 rounded-md p-3 shrink-0">
          <div className="flex items-center space-x-1.5 text-xs font-semibold text-zinc-400 mb-1.5">
            <MessageSquare className="w-3.5 h-3.5 text-zinc-500" />
            <span>Pertanyaan (Transkrip)</span>
          </div>
          {qaState.question ? (
            <p className="text-zinc-200 text-sm font-medium leading-snug">
              {qaState.question}
            </p>
          ) : (
            <p className="text-zinc-600 text-xs italic">
              Belum ada pertanyaan aktif. Tahan tombol F8 dan ucapkan pertanyaan viewer.
            </p>
          )}
        </div>

        {/* Answer Panel */}
        <div className="flex-1 bg-zinc-900/40 border border-zinc-800/60 rounded-md p-3 overflow-y-auto min-h-0">
          <div className="flex items-center space-x-1.5 text-xs font-semibold text-zinc-400 mb-2">
            <Sparkles className="w-3.5 h-3.5 text-purple-400" />
            <span>Jawaban AI</span>
          </div>
          {qaState.answer ? (
            <div className="markdown-body prose prose-invert max-w-none text-zinc-100 leading-relaxed text-sm">
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                rehypePlugins={[rehypeHighlight]}
                components={markdownComponents}
              >
                {qaState.answer}
              </ReactMarkdown>
            </div>
          ) : qaState.status === "answering" ? (
            <div className="flex items-center space-x-2 text-xs text-zinc-500 py-4 justify-center">
              <Sparkles className="w-4 h-4 animate-spin text-purple-400" />
              <span>Memproses jawaban AI...</span>
            </div>
          ) : (
            <div className="flex flex-col items-center justify-center h-32 text-center text-zinc-600 text-xs">
              <p>Jawaban ringkas AI akan ditampilkan di sini.</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
