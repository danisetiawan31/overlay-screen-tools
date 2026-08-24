import { render, screen, waitFor, fireEvent, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { listen } from "@tauri-apps/api/event";
import App from "./App";
import { commands } from "./bindings";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

vi.mock("./bindings", () => ({
  commands: {
    getAppState: vi.fn(),
    updateFontSize: vi.fn(),
    getNotesState: vi.fn().mockResolvedValue({
      status: "ok",
      data: { content: null, error: null },
    }),
    pickNotesFile: vi.fn().mockResolvedValue({
      status: "ok",
      data: null,
    }),
    sendAudioBlob: vi.fn().mockResolvedValue({
      status: "ok",
      data: null,
    }),
  },
  events: {
    notesUpdate: {
      listen: vi.fn().mockResolvedValue(vi.fn()),
    },
    notesError: {
      listen: vi.fn().mockResolvedValue(vi.fn()),
    },
    qaRecordingEnded: {
      listen: vi.fn().mockResolvedValue(vi.fn()),
    },
    transcriptResult: {
      listen: vi.fn().mockResolvedValue(vi.fn()),
    },
    answerResult: {
      listen: vi.fn().mockResolvedValue(vi.fn()),
    },
    qaError: {
      listen: vi.fn().mockResolvedValue(vi.fn()),
    },
  },
}));

describe("App Component", () => {
  let mockUnlisten: () => void;
  // eslint-disable-next-line @typescript-eslint/no-unsafe-function-type
  let recordingStartedCallbacks: Function[] = [];

  beforeEach(() => {
    vi.clearAllMocks();
    mockUnlisten = vi.fn();
    recordingStartedCallbacks = [];

    vi.mocked(listen).mockImplementation((event, cb) => {
      if (event === "qa:recording-started") {
        recordingStartedCallbacks.push(cb);
      }
      return Promise.resolve(mockUnlisten);
    });

    vi.mocked(commands.getNotesState).mockResolvedValue({
      status: "ok",
      data: { content: null, error: null },
    });
  });

  it("renders loading state initially before getAppState resolves", () => {
    vi.mocked(commands.getAppState).mockReturnValue(new Promise(() => {}));

    render(<App />);
    expect(screen.getByText(/memuat status aplikasi/i)).toBeInTheDocument();
  });

  it("loads app state successfully and renders Notes tab by default", async () => {
    vi.mocked(commands.getAppState).mockResolvedValue({
      status: "ok",
      data: { fontSize: 16, qaAvailable: true },
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("16px")).toBeInTheDocument();
    });

    expect(screen.getByRole("tabpanel", { name: /^notes$/i })).toBeInTheDocument();
    expect(screen.getByText("File Mode")).toBeInTheDocument();
  });

  it("switches tabs manually between Notes and Live Q&A when qaAvailable is true", async () => {
    vi.mocked(commands.getAppState).mockResolvedValue({
      status: "ok",
      data: { fontSize: 14, qaAvailable: true },
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("14px")).toBeInTheDocument();
    });

    const qaTab = screen.getByRole("tab", { name: /^live q&a$/i });
    fireEvent.click(qaTab);

    expect(screen.getByRole("tabpanel", { name: /^live q&a$/i })).toBeInTheDocument();
    expect(screen.getByText(/Siap — Tahan/i)).toBeInTheDocument();

    const notesTab = screen.getByRole("tab", { name: /^notes$/i });
    fireEvent.click(notesTab);

    expect(screen.getByRole("tabpanel", { name: /^notes$/i })).toBeInTheDocument();
    expect(screen.getByText("File Mode")).toBeInTheDocument();
  });

  it("disables Live Q&A tab when qaAvailable is false", async () => {
    vi.mocked(commands.getAppState).mockResolvedValue({
      status: "ok",
      data: { fontSize: 14, qaAvailable: false },
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText("14px")).toBeInTheDocument();
    });

    const qaTab = screen.getByRole("tab", { name: /live q&a/i });
    expect(qaTab).toBeDisabled();

    fireEvent.click(qaTab);
    expect(screen.getByRole("tabpanel", { name: /^notes$/i })).toBeInTheDocument();
  });

  it("auto-switches to Live Q&A tab on qa:recording-started event when qaAvailable is true", async () => {
    vi.mocked(commands.getAppState).mockResolvedValue({
      status: "ok",
      data: { fontSize: 14, qaAvailable: true },
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByRole("tabpanel", { name: /^notes$/i })).toBeInTheDocument();
    });

    await waitFor(() => {
      expect(recordingStartedCallbacks.length).toBeGreaterThan(0);
    });

    act(() => {
      recordingStartedCallbacks.forEach((cb) =>
        cb({ event: "qa:recording-started", id: 1, payload: undefined })
      );
    });

    const qaPanel = screen.getByRole("tabpanel", { name: /^live q&a$/i });
    expect(qaPanel).not.toHaveClass("hidden");
    const notesPanel = screen.getByRole("tabpanel", { name: /^notes$/i });
    expect(notesPanel).toHaveClass("hidden");
  });

  it("ignores qa:recording-started event (defensive check) when qaAvailable is false", async () => {
    vi.mocked(commands.getAppState).mockResolvedValue({
      status: "ok",
      data: { fontSize: 14, qaAvailable: false },
    });

    render(<App />);

    await waitFor(() => {
      expect(screen.getByRole("tabpanel", { name: /^notes$/i })).toBeInTheDocument();
    });

    await waitFor(() => {
      expect(recordingStartedCallbacks.length).toBeGreaterThan(0);
    });

    act(() => {
      recordingStartedCallbacks.forEach((cb) =>
        cb({ event: "qa:recording-started", id: 1, payload: undefined })
      );
    });

    // Tetap di tab Notes
    const notesPanel = screen.getByRole("tabpanel", { name: /^notes$/i });
    expect(notesPanel).not.toHaveClass("hidden");
  });

  it("handles race condition correctly using ref (responds to latest qaAvailable after deferred getAppState resolve)", async () => {
    let resolveAppState!: (val: { status: "ok"; data: { fontSize: number; qaAvailable: boolean } }) => void;
    const appStatePromise = new Promise<{ status: "ok"; data: { fontSize: number; qaAvailable: boolean } }>((res) => {
      resolveAppState = res;
    });

    vi.mocked(commands.getAppState).mockReturnValue(appStatePromise);

    render(<App />);

    // Tunggu listener terpasang
    await waitFor(() => {
      expect(recordingStartedCallbacks.length).toBeGreaterThan(0);
    });

    // Fire event saat getAppState masih pending (qaAvailable masih null/false)
    act(() => {
      recordingStartedCallbacks.forEach((cb) =>
        cb({ event: "qa:recording-started", id: 1, payload: undefined })
      );
    });

    // Selesaikan resolve getAppState ke qaAvailable: true
    await act(async () => {
      resolveAppState({
        status: "ok",
        data: { fontSize: 14, qaAvailable: true },
      });
    });

    await waitFor(() => {
      expect(screen.getByRole("tabpanel", { name: /^notes$/i })).toBeInTheDocument();
    });

    // Fire event lagi setelah resolve -> ref terbaca true dan berpindah ke Live Q&A
    act(() => {
      recordingStartedCallbacks.forEach((cb) =>
        cb({ event: "qa:recording-started", id: 2, payload: undefined })
      );
    });

    const qaPanel = screen.getByRole("tabpanel", { name: /^live q&a$/i });
    expect(qaPanel).not.toHaveClass("hidden");
  });

  it("cleans up unlisten listener on unmount", async () => {
    vi.mocked(commands.getAppState).mockResolvedValue({
      status: "ok",
      data: { fontSize: 14, qaAvailable: true },
    });

    const { unmount } = render(<App />);

    await waitFor(() => {
      expect(listen).toHaveBeenCalledWith("qa:recording-started", expect.any(Function));
    });

    // Beri waktu microtask promise listen resolve
    await waitFor(() => {
      expect(recordingStartedCallbacks.length).toBeGreaterThan(0);
    });

    unmount();

    expect(mockUnlisten).toHaveBeenCalled();
  });
});
