import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor, act, fireEvent } from "@testing-library/react";
import { listen } from "@tauri-apps/api/event";
import App from "./App";
import { commands, events } from "./bindings";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

describe("Live Q&A Mode — Full Pipeline E2E Integration Tests", () => {
  // eslint-disable-next-line @typescript-eslint/no-unsafe-function-type
  let recordingStartedCbs: Function[] = [];
  // eslint-disable-next-line @typescript-eslint/no-unsafe-function-type
  let recordingEndedCbs: Function[] = [];
  // eslint-disable-next-line @typescript-eslint/no-unsafe-function-type
  let transcriptResultCbs: Function[] = [];
  // eslint-disable-next-line @typescript-eslint/no-unsafe-function-type
  let answerResultCbs: Function[] = [];
  // eslint-disable-next-line @typescript-eslint/no-unsafe-function-type
  let qaErrorCbs: Function[] = [];

  const triggerRecordingStarted = async () => {
    for (const cb of recordingStartedCbs) {
      await cb({ event: "qa:recording-started", id: 1, payload: undefined });
    }
  };

  const triggerRecordingEnded = async (payload: { belowThreshold: boolean }) => {
    for (const cb of recordingEndedCbs) {
      await cb({ payload });
    }
  };

  const triggerTranscriptResult = async (payload: { text: string }) => {
    for (const cb of transcriptResultCbs) {
      await cb({ payload });
    }
  };

  const triggerAnswerResult = async (payload: { text: string }) => {
    for (const cb of answerResultCbs) {
      await cb({ payload });
    }
  };

  const triggerQaError = async (payload: { stage: string; message: string }) => {
    for (const cb of qaErrorCbs) {
      await cb({ payload });
    }
  };

  let mockRecorderInstance: {
    start: ReturnType<typeof vi.fn>;
    stop: ReturnType<typeof vi.fn>;
    state: string;
    // eslint-disable-next-line @typescript-eslint/no-unsafe-function-type
    ondataavailable: Function | null;
    // eslint-disable-next-line @typescript-eslint/no-unsafe-function-type
    onstop: Function | null;
  };

  beforeEach(() => {
    vi.clearAllMocks();
    recordingStartedCbs = [];
    recordingEndedCbs = [];
    transcriptResultCbs = [];
    answerResultCbs = [];
    qaErrorCbs = [];

    // Mock App state
    vi.spyOn(commands, "getAppState").mockResolvedValue({
      status: "ok",
      data: { fontSize: 14, qaAvailable: true },
    });

    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: { content: null, error: null },
    });

    vi.spyOn(commands, "sendAudioBlob").mockResolvedValue({
      status: "ok",
      data: null,
    });

    // Mock Tauri event listeners
    vi.mocked(listen).mockImplementation((event, cb) => {
      if (event === "qa:recording-started") {
        recordingStartedCbs.push(cb);
      }
      return Promise.resolve(vi.fn());
    });

    vi.spyOn(events.qaRecordingEnded, "listen").mockImplementation((cb) => {
      recordingEndedCbs.push(cb);
      return Promise.resolve(vi.fn());
    });

    vi.spyOn(events.transcriptResult, "listen").mockImplementation((cb) => {
      transcriptResultCbs.push(cb);
      return Promise.resolve(vi.fn());
    });

    vi.spyOn(events.answerResult, "listen").mockImplementation((cb) => {
      answerResultCbs.push(cb);
      return Promise.resolve(vi.fn());
    });

    vi.spyOn(events.qaError, "listen").mockImplementation((cb) => {
      qaErrorCbs.push(cb);
      return Promise.resolve(vi.fn());
    });

    vi.spyOn(events.notesUpdate, "listen").mockResolvedValue(vi.fn());
    vi.spyOn(events.notesError, "listen").mockResolvedValue(vi.fn());

    // Mock MediaRecorder & navigator.mediaDevices
    mockRecorderInstance = {
      start: vi.fn(() => {
        mockRecorderInstance.state = "recording";
      }),
      stop: vi.fn(() => {
        mockRecorderInstance.state = "inactive";
        if (mockRecorderInstance.onstop) {
          mockRecorderInstance.onstop();
        }
      }),
      state: "inactive",
      ondataavailable: null,
      onstop: null,
    };

    class MockMediaRecorder {
      start = mockRecorderInstance.start;
      stop = mockRecorderInstance.stop;
      get state() {
        return mockRecorderInstance.state;
      }
      set ondataavailable(cb) {
        mockRecorderInstance.ondataavailable = cb;
      }
      get ondataavailable() {
        return mockRecorderInstance.ondataavailable;
      }
      set onstop(cb) {
        mockRecorderInstance.onstop = cb;
      }
      get onstop() {
        return mockRecorderInstance.onstop;
      }
      static isTypeSupported = vi.fn().mockReturnValue(true);
    }

    vi.stubGlobal("MediaRecorder", MockMediaRecorder);

    const mockTrack = { stop: vi.fn() };
    const mockStream = {
      active: true,
      getTracks: () => [mockTrack],
    };

    vi.stubGlobal("navigator", {
      mediaDevices: {
        getUserMedia: vi.fn().mockResolvedValue(mockStream),
      },
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("1. Full pipeline sukses: F8 pressed (>400ms) -> auto-switch -> transcript -> answer", async () => {
    render(<App />);

    // 1. Initial render di Notes tab
    await waitFor(() => {
      expect(screen.getByRole("tabpanel", { name: /^notes$/i })).toBeInTheDocument();
    });

    // 2. Trigger F8 Press (E2E simulasi) -> emit qa:recording-started
    await act(async () => {
      await triggerRecordingStarted();
    });

    // Auto-switch ke tab Q&A dan mulai merekam
    expect(screen.getByRole("tabpanel", { name: /^live q&a$/i })).toBeInTheDocument();
    expect(screen.getByText(/Merekam suara.../i)).toBeInTheDocument();
    expect(mockRecorderInstance.start).toHaveBeenCalledTimes(1);

    // Kirim chunk audio palsu
    const fakeChunk = new Blob(["audio-data-test"], { type: "audio/webm" });
    if (mockRecorderInstance.ondataavailable) {
      mockRecorderInstance.ondataavailable({ data: fakeChunk });
    }

    // 3. Trigger F8 Release (>400ms hold) -> emit qa:recording-ended { belowThreshold: false }
    await act(async () => {
      await triggerRecordingEnded({ belowThreshold: false });
    });

    expect(commands.sendAudioBlob).toHaveBeenCalledTimes(1);
    expect(screen.getByText(/Mentranskripsi suara/i)).toBeInTheDocument();

    // 4. Emisi transcript:result (Groq STT selesai, non-blocking sebelum AI selesai)
    await act(async () => {
      await triggerTranscriptResult({
        text: "Bagaimana arsitektur memory model di Rust?",
      });
    });

    expect(screen.getByText("Bagaimana arsitektur memory model di Rust?")).toBeInTheDocument();
    expect(screen.getByText(/Menghasilkan contekan AI/i)).toBeInTheDocument();

    // 5. Emisi answer:result (OpenRouter AI selesai)
    await act(async () => {
      await triggerAnswerResult({
        text: "### Memory Model Rust\n- **Ownership & Borrowing**: Menjamin memory safety tanpa GC\n- **RAII**: Resource dibebaskan otomatis saat out of scope",
      });
    });

    expect(screen.getByRole("heading", { level: 3, name: "Memory Model Rust" })).toBeInTheDocument();
    expect(screen.getByText(/Ownership & Borrowing/i)).toBeInTheDocument();
    expect(screen.getByText(/Siap — Tahan/i)).toBeInTheDocument();
  });

  it("2. Discard stale request: F8 dipicu ulang saat request sebelumnya masih berjalan", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByRole("tabpanel", { name: /^notes$/i })).toBeInTheDocument();
    });

    // Request 1: Start & End
    await act(async () => {
      await triggerRecordingStarted();
    });

    const fakeChunk1 = new Blob(["audio-req-1"], { type: "audio/webm" });
    if (mockRecorderInstance.ondataavailable) {
      mockRecorderInstance.ondataavailable({ data: fakeChunk1 });
    }

    await act(async () => {
      await triggerRecordingEnded({ belowThreshold: false });
    });

    expect(commands.sendAudioBlob).toHaveBeenCalledTimes(1);

    // Sebelum Request 1 selesai, user menekan F8 lagi untuk pertanyaan baru (Request 2)
    await act(async () => {
      await triggerRecordingStarted();
    });

    expect(screen.getByText(/Merekam suara.../i)).toBeInTheDocument();

    const fakeChunk2 = new Blob(["audio-req-2"], { type: "audio/webm" });
    if (mockRecorderInstance.ondataavailable) {
      mockRecorderInstance.ondataavailable({ data: fakeChunk2 });
    }

    await act(async () => {
      await triggerRecordingEnded({ belowThreshold: false });
    });

    expect(commands.sendAudioBlob).toHaveBeenCalledTimes(2);

    // Event Request 2 (generasi terbaru) tiba
    await act(async () => {
      await triggerTranscriptResult({ text: "Pertanyaan Generasi Terbaru" });
      await triggerAnswerResult({ text: "Jawaban Generasi Terbaru" });
    });

    // UI hanya menampilkan konten dari request terbaru
    expect(screen.getByText("Pertanyaan Generasi Terbaru")).toBeInTheDocument();
    expect(screen.getByText("Jawaban Generasi Terbaru")).toBeInTheDocument();
    expect(screen.queryByText("audio-req-1")).not.toBeInTheDocument();
  });

  it("3. Hold di bawah threshold (<400ms): audio dibuang diam-diam, tidak invoke command", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByRole("tabpanel", { name: /^notes$/i })).toBeInTheDocument();
    });

    // Quick press F8
    await act(async () => {
      await triggerRecordingStarted();
    });

    expect(mockRecorderInstance.start).toHaveBeenCalledTimes(1);

    // Quick release F8 (<400ms)
    await act(async () => {
      await triggerRecordingEnded({ belowThreshold: true });
    });

    // Tidak ada pemanggilan command backend
    expect(commands.sendAudioBlob).not.toHaveBeenCalled();
    expect(screen.getByText(/Siap — Tahan/i)).toBeInTheDocument();
  });

  it("4. Mic permission ditolak: error banner lokal muncul, send_audio_blob tidak pernah dipanggil", async () => {
    vi.stubGlobal("navigator", {
      mediaDevices: {
        getUserMedia: vi.fn().mockRejectedValue(new Error("NotAllowedError: permission denied")),
      },
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByRole("tabpanel", { name: /^notes$/i })).toBeInTheDocument();
    });

    // F8 Press
    await act(async () => {
      await triggerRecordingStarted();
    });

    // Banner error lokal muncul
    const alert = screen.getByRole("alert");
    expect(alert).toBeInTheDocument();
    expect(alert).toHaveTextContent(/Mikrofon: NotAllowedError: permission denied/i);

    // F8 Release
    await act(async () => {
      await triggerRecordingEnded({ belowThreshold: false });
    });

    // Tidak pernah menyentuh core command
    expect(commands.sendAudioBlob).not.toHaveBeenCalled();
  });

  it("5. Semua fallback key habis: core emit qa:error dan UI menampilkan pesan error", async () => {
    render(<App />);

    await waitFor(() => {
      expect(screen.getByRole("tabpanel", { name: /^notes$/i })).toBeInTheDocument();
    });

    // Tab ke Q&A
    const qaTab = screen.getByRole("tab", { name: /^live q&a$/i });
    fireEvent.click(qaTab);

    // Simulasi core memancarkan qa:error setelah semua fallback key gagal
    await act(async () => {
      await triggerQaError({
        stage: "stt",
        message: "HTTP 429 Too Many Requests: Rate limit exceeded on all Groq keys",
      });
    });

    const alert = screen.getByRole("alert");
    expect(alert).toBeInTheDocument();
    expect(alert).toHaveTextContent(
      "[STT] HTTP 429 Too Many Requests: Rate limit exceeded on all Groq keys"
    );

    // Tombol close menutup banner error
    const closeBtn = screen.getByRole("button", { name: "Tutup error" });
    fireEvent.click(closeBtn);

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
