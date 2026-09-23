import React, { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import {
  Mic,
  MicOff,
  AlertCircle,
  MessageSquare,
  Sparkles,
  Radio,
  SendHorizontal,
  Copy,
  Check,
} from "lucide-react";
import { commands, events } from "../bindings";
import { markdownComponents, copyTextToClipboard } from "./MermaidRenderer";

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
  const [promptInput, setPromptInput] = useState("");
  const [answerCopied, setAnswerCopied] = useState(false);
  const [selectedText, setSelectedText] = useState("");
  const [selectionCopied, setSelectionCopied] = useState(false);
  const [floatingPos, setFloatingPos] = useState<{ x: number; y: number } | null>(null);

  const answerContainerRef = useRef<HTMLDivElement | null>(null);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const audioStreamRef = useRef<MediaStream | null>(null);
  const audioChunksRef = useRef<Blob[]>([]);
  const isRecordingRef = useRef<boolean>(false);

  const isProcessing =
    qaState.status === "recording" ||
    qaState.status === "transcribing" ||
    qaState.status === "answering";

  const handleCopyAnswer = async () => {
    if (!qaState.answer) return;
    const ok = await copyTextToClipboard(qaState.answer);
    if (ok) {
      setAnswerCopied(true);
      setTimeout(() => setAnswerCopied(false), 2000);
    }
  };

  const handleSelectionMouseUp = () => {
    const sel = window.getSelection();
    const text = sel ? sel.toString().trim() : "";
    if (text.length > 0 && answerContainerRef.current) {
      const range = sel?.rangeCount ? sel.getRangeAt(0) : null;
      if (range) {
        const rect = range.getBoundingClientRect();
        if (rect.width > 0 || rect.height > 0) {
          setSelectedText(text);
          setSelectionCopied(false);
          setFloatingPos({
            x: Math.min(
              Math.max(10, rect.left + rect.width / 2 - 50),
              window.innerWidth - 130
            ),
            y: Math.max(10, rect.top - 36),
          });
          return;
        }
      }
    }
    setFloatingPos(null);
    setSelectedText("");
  };

  const handleCopySelectedText = async () => {
    if (!selectedText) return;
    const ok = await copyTextToClipboard(selectedText);
    if (ok) {
      setSelectionCopied(true);
      setTimeout(() => {
        setSelectionCopied(false);
        setFloatingPos(null);
        setSelectedText("");
      }, 1500);
    }
  };

  useEffect(() => {
    const handleCopyEvent = () => {
      const sel = window.getSelection();
      const text = sel ? sel.toString().trim() : "";
      if (text) {
        copyTextToClipboard(text);
      }
    };
    window.addEventListener("copy", handleCopyEvent);
    return () => window.removeEventListener("copy", handleCopyEvent);
  }, []);

  const handleSubmitPrompt = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = promptInput.trim();
    if (!trimmed || isProcessing) return;

    setPromptInput("");
    setQaState((prev) => ({
      ...prev,
      question: trimmed,
      answer: null,
      status: "answering",
      error: null,
    }));

    try {
      const res = await commands.askAiText({ prompt: trimmed });
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
        error: `Gagal mengirim pertanyaan: ${msg}`,
      }));
    }
  };

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
      <div className="h-full flex flex-col items-center justify-center p-6 text-center text-[#71717a] bg-[#1a1a1d]/50 rounded-lg border border-[#38383e]">
        <MicOff className="w-8 h-8 mb-2 text-[#71717a]" />
        <h3 className="font-semibold text-[#a1a1aa] text-sm mb-1">Live Q&A Tidak Tersedia</h3>
        <p className="text-xs max-w-sm">
          Hotkey F8 gagal didaftarkan pada sesi ini. Fitur push-to-talk dinonaktifkan secara otomatis.
        </p>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col space-y-3 overflow-hidden select-text">
      {/* Status Bar */}
      <div className="flex items-center justify-between px-3 py-2 bg-[#28282d] rounded-md border border-[#38383e] text-xs">
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
            <span className="flex items-center space-x-1.5 text-[#a1a1aa]">
              <Mic className="w-3.5 h-3.5 text-emerald-400" />
              <span>Siap — Tahan <kbd className="px-1 py-0.5 bg-[#1e1e20] rounded border border-[#38383e] text-[#d4d4d8] font-mono">F8</kbd> untuk bertanya</span>
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
          className="bg-red-950/70 border border-red-800/80 px-3 py-2 rounded text-xs text-red-300 flex items-start justify-between"
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
        <div className="bg-[#242428] border border-[#38383e] rounded-md p-3 shrink-0">
          <div className="flex items-center space-x-1.5 text-xs font-semibold text-[#a1a1aa] mb-1.5">
            <MessageSquare className="w-3.5 h-3.5 text-[#71717a]" />
            <span>Pertanyaan (Transkrip)</span>
          </div>
          {qaState.question ? (
            <p className="text-[#fafafa] text-sm font-medium leading-snug">
              {qaState.question}
            </p>
          ) : (
            <p className="text-[#71717a] text-xs italic">
              Belum ada pertanyaan aktif. Ketik pertanyaan di bawah atau tahan tombol F8 untuk bicara.
            </p>
          )}
        </div>

        {/* Answer Panel */}
        <div
          ref={answerContainerRef}
          onMouseUp={handleSelectionMouseUp}
          className="relative flex-1 bg-[#28282d] border border-[#38383e] rounded-md p-3 overflow-y-auto min-h-0 select-text"
        >
          <div className="flex items-center justify-between mb-2">
            <div className="flex items-center space-x-1.5 text-xs font-semibold text-[#a1a1aa]">
              <Sparkles className="w-3.5 h-3.5 text-purple-400" />
              <span>Jawaban AI</span>
            </div>
            {qaState.answer && (
              <button
                type="button"
                onClick={handleCopyAnswer}
                className="flex items-center gap-1.5 px-2.5 py-1 rounded bg-[#34343a] hover:bg-[#404048] active:bg-[#4a4a52] text-[#d4d4d8] hover:text-white text-xs font-medium transition border border-[#3f3f46] shadow-xs"
                title="Salin seluruh jawaban AI ke clipboard"
                aria-label="Salin jawaban"
              >
                {answerCopied ? (
                  <>
                    <Check className="w-3.5 h-3.5 text-emerald-400" />
                    <span className="text-emerald-400">Tersalin!</span>
                  </>
                ) : (
                  <>
                    <Copy className="w-3.5 h-3.5" />
                    <span>Salin Jawaban</span>
                  </>
                )}
              </button>
            )}
          </div>
          {qaState.answer ? (
            <div className="markdown-body prose prose-invert max-w-none text-[#d4d4d8] leading-relaxed text-sm break-words space-y-2.5 select-text [&_strong]:text-white [&_strong]:font-bold [&_h1]:text-base [&_h1]:font-bold [&_h1]:text-[#fafafa] [&_h2]:text-sm [&_h2]:font-semibold [&_h2]:text-[#fafafa] [&_h3]:text-xs [&_h3]:font-medium [&_h3]:text-[#f4f4f5] [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:list-decimal [&_ol]:pl-5 [&_table]:border-collapse [&_th]:border [&_th]:border-[#38383e] [&_th]:p-1.5 [&_th]:bg-[#242428] [&_th]:text-[#fafafa] [&_td]:border [&_td]:border-[#38383e] [&_td]:p-1.5 [&_td]:text-[#d4d4d8] [&_blockquote]:border-l-[3px] [&_blockquote]:border-l-[#585862] [&_blockquote]:border-t-0 [&_blockquote]:border-r-0 [&_blockquote]:border-b-0 [&_blockquote]:bg-[#28282d] [&_blockquote]:px-4 [&_blockquote]:py-2.5 [&_blockquote]:rounded-r-md [&_blockquote]:my-2.5 [&_blockquote]:text-[#d4d4d8] [&_blockquote]:italic [&_p]:my-1.5">
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                rehypePlugins={[rehypeHighlight]}
                components={markdownComponents}
              >
                {qaState.answer}
              </ReactMarkdown>
            </div>
          ) : qaState.status === "answering" ? (
            <div className="flex items-center space-x-2 text-xs text-[#71717a] py-4 justify-center">
              <Sparkles className="w-4 h-4 animate-spin text-purple-400" />
              <span>Memproses jawaban AI...</span>
            </div>
          ) : (
            <div className="flex flex-col items-center justify-center h-32 text-center text-[#71717a] text-xs">
              <p>Jawaban ringkas AI akan ditampilkan di sini.</p>
            </div>
          )}
        </div>
      </div>

      {/* Floating Copy Button for highlighted text selection */}
      {floatingPos && selectedText && (
        <div
          style={{
            position: "fixed",
            left: `${floatingPos.x}px`,
            top: `${floatingPos.y}px`,
            zIndex: 9999,
          }}
          className="animate-in fade-in zoom-in-95 duration-150"
        >
          <button
            type="button"
            onClick={handleCopySelectedText}
            className="flex items-center gap-1 px-2.5 py-1 rounded-full bg-purple-600 hover:bg-purple-500 text-white text-xs font-medium shadow-lg shadow-purple-950/50 border border-purple-400/30 transition active:scale-95 cursor-pointer"
            title="Salin teks yang disorot"
            aria-label="Salin teks terpilih"
          >
            {selectionCopied ? (
              <>
                <Check className="w-3.5 h-3.5 text-emerald-300" />
                <span>Tersalin!</span>
              </>
            ) : (
              <>
                <Copy className="w-3.5 h-3.5" />
                <span>Salin ({selectedText.length > 15 ? `${selectedText.length} char` : "Teks"})</span>
              </>
            )}
          </button>
        </div>
      )}

      {/* Text Prompt Input Bar (Single-line) */}
      <form
        onSubmit={handleSubmitPrompt}
        className="shrink-0 flex items-center gap-2 pt-1 border-t border-[#2e2e32]"
      >
        <div className="relative flex-1">
          <input
            type="text"
            value={promptInput}
            onChange={(e) => setPromptInput(e.target.value)}
            placeholder="Tanya AI atau tahan F8 untuk bicara..."
            disabled={isProcessing}
            className="w-full bg-[#242428] border border-[#38383e] focus:border-purple-500/80 focus:ring-1 focus:ring-purple-500/40 rounded-md px-3 py-1.5 text-xs text-[#fafafa] placeholder:text-[#71717a] outline-none transition disabled:opacity-50 disabled:cursor-not-allowed"
            aria-label="Prompt pertanyaan AI"
          />
        </div>
        <button
          type="submit"
          disabled={isProcessing || !promptInput.trim()}
          className="inline-flex items-center justify-center p-1.5 rounded-md bg-purple-600 hover:bg-purple-500 active:bg-purple-700 text-white disabled:opacity-40 disabled:hover:bg-purple-600 disabled:cursor-not-allowed transition shrink-0"
          aria-label="Kirim pertanyaan"
          title="Kirim (Enter)"
        >
          <SendHorizontal className="w-3.5 h-3.5" />
        </button>
      </form>
    </div>
  );
};
