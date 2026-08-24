import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act, fireEvent } from "@testing-library/react";
import { listen } from "@tauri-apps/api/event";
import { QaPanel } from "./QaPanel";
import { commands, events } from "../bindings";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

describe("QaPanel Component", () => {
  // eslint-disable-next-line @typescript-eslint/no-unsafe-function-type
  let recordingStartedCb: Function | null = null;
  // eslint-disable-next-line @typescript-eslint/no-unsafe-function-type
  let recordingEndedCb: Function | null = null;
  // eslint-disable-next-line @typescript-eslint/no-unsafe-function-type
  let transcriptResultCb: Function | null = null;
  // eslint-disable-next-line @typescript-eslint/no-unsafe-function-type
  let answerResultCb: Function | null = null;
  // eslint-disable-next-line @typescript-eslint/no-unsafe-function-type
  let qaErrorCb: Function | null = null;

  // Mock MediaRecorder
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
    recordingStartedCb = null;
    recordingEndedCb = null;
    transcriptResultCb = null;
    answerResultCb = null;
    qaErrorCb = null;

    vi.mocked(listen).mockImplementation((event, cb) => {
      if (event === "qa:recording-started") {
        recordingStartedCb = cb;
      }
      return Promise.resolve(vi.fn());
    });

    vi.spyOn(events.qaRecordingEnded, "listen").mockImplementation((cb) => {
      recordingEndedCb = cb;
      return Promise.resolve(vi.fn());
    });

    vi.spyOn(events.transcriptResult, "listen").mockImplementation((cb) => {
      transcriptResultCb = cb;
      return Promise.resolve(vi.fn());
    });

    vi.spyOn(events.answerResult, "listen").mockImplementation((cb) => {
      answerResultCb = cb;
      return Promise.resolve(vi.fn());
    });

    vi.spyOn(events.qaError, "listen").mockImplementation((cb) => {
      qaErrorCb = cb;
      return Promise.resolve(vi.fn());
    });

    vi.spyOn(commands, "sendAudioBlob").mockResolvedValue({
      status: "ok",
      data: null,
    });

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

    // Setup global Mock MediaRecorder
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

    // Mock navigator.mediaDevices
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

  it("1. renders idle state by default when qaAvailable is true", () => {
    render(<QaPanel qaAvailable={true} />);

    expect(screen.getByText(/Siap — Tahan/i)).toBeInTheDocument();
    expect(screen.getByText(/Belum ada pertanyaan aktif/i)).toBeInTheDocument();
    expect(screen.getByText(/Jawaban ringkas AI akan ditampilkan di sini/i)).toBeInTheDocument();
  });

  it("2. renders disabled state when qaAvailable is false", () => {
    render(<QaPanel qaAvailable={false} />);

    expect(screen.getByText("Live Q&A Tidak Tersedia")).toBeInTheDocument();
    expect(screen.getByText(/Hotkey F8 gagal didaftarkan/i)).toBeInTheDocument();
  });

  it("3. handles qa:recording-started event and starts MediaRecorder", async () => {
    render(<QaPanel qaAvailable={true} />);

    expect(recordingStartedCb).toBeDefined();

    await act(async () => {
      if (recordingStartedCb) {
        await recordingStartedCb();
      }
    });

    expect(navigator.mediaDevices.getUserMedia).toHaveBeenCalledWith({ audio: true });
    expect(mockRecorderInstance.start).toHaveBeenCalled();
    expect(screen.getByText(/Merekam suara.../i)).toBeInTheDocument();
  });

  it("4. handles mic permission rejection on qa:recording-started and renders local error", async () => {
    vi.stubGlobal("navigator", {
      mediaDevices: {
        getUserMedia: vi.fn().mockRejectedValue(new Error("Permission denied by user")),
      },
    });

    render(<QaPanel qaAvailable={true} />);

    await act(async () => {
      if (recordingStartedCb) {
        await recordingStartedCb();
      }
    });

    const alert = screen.getByRole("alert");
    expect(alert).toBeInTheDocument();
    expect(alert).toHaveTextContent(/Mikrofon: Permission denied by user/i);
    expect(commands.sendAudioBlob).not.toHaveBeenCalled();
  });

  it("5. handles qa:recording-ended with belowThreshold=true by discarding silently", async () => {
    render(<QaPanel qaAvailable={true} />);

    // Start recording first
    await act(async () => {
      if (recordingStartedCb) {
        await recordingStartedCb();
      }
    });

    expect(mockRecorderInstance.start).toHaveBeenCalled();

    // Release F8 below threshold (quick tap)
    await act(async () => {
      if (recordingEndedCb) {
        await recordingEndedCb({ payload: { belowThreshold: true } });
      }
    });

    expect(commands.sendAudioBlob).not.toHaveBeenCalled();
    expect(screen.getByText(/Siap — Tahan/i)).toBeInTheDocument();
  });

  it("6. handles qa:recording-ended with belowThreshold=false by sending audio blob", async () => {
    render(<QaPanel qaAvailable={true} />);

    // Start recording
    await act(async () => {
      if (recordingStartedCb) {
        await recordingStartedCb();
      }
    });

    // Provide mock audio chunk
    const fakeChunk = new Blob(["fake-audio-bytes"], { type: "audio/webm" });
    if (mockRecorderInstance.ondataavailable) {
      mockRecorderInstance.ondataavailable({ data: fakeChunk });
    }

    // Release F8 above threshold
    await act(async () => {
      if (recordingEndedCb) {
        await recordingEndedCb({ payload: { belowThreshold: false } });
      }
    });

    expect(commands.sendAudioBlob).toHaveBeenCalledTimes(1);
    expect(screen.getByText(/Mentranskripsi suara/i)).toBeInTheDocument();
  });

  it("7. updates question when transcript:result event arrives", async () => {
    render(<QaPanel qaAvailable={true} />);

    await act(async () => {
      if (transcriptResultCb) {
        transcriptResultCb({ payload: { text: "Bagaimana cara kerja Rust ownership?" } });
      }
    });

    expect(screen.getByText("Bagaimana cara kerja Rust ownership?")).toBeInTheDocument();
    expect(screen.getByText(/Menghasilkan contekan AI/i)).toBeInTheDocument();
  });

  it("8. updates markdown answer when answer:result event arrives", async () => {
    render(<QaPanel qaAvailable={true} />);

    await act(async () => {
      if (answerResultCb) {
        answerResultCb({
          payload: {
            text: "### Ownership di Rust\n- Setiap nilai punya 1 owner\n- Ketika owner keluar scope, memori dibebaskan",
          },
        });
      }
    });

    expect(screen.getByRole("heading", { level: 3, name: "Ownership di Rust" })).toBeInTheDocument();
    expect(screen.getByText("Setiap nilai punya 1 owner")).toBeInTheDocument();
    expect(screen.getByText(/Siap — Tahan/i)).toBeInTheDocument();
  });

  it("9. renders qa:error event from core and allows dismissal", async () => {
    render(<QaPanel qaAvailable={true} />);

    await act(async () => {
      if (qaErrorCb) {
        qaErrorCb({ payload: { stage: "stt", message: "HTTP 429 Too Many Requests" } });
      }
    });

    const alert = screen.getByRole("alert");
    expect(alert).toBeInTheDocument();
    expect(alert).toHaveTextContent("[STT] HTTP 429 Too Many Requests");

    const closeBtn = screen.getByRole("button", { name: "Tutup error" });
    fireEvent.click(closeBtn);

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
