import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import { NotesPanel } from "./NotesPanel";
import { commands, events } from "../bindings";

describe("NotesPanel Component", () => {
  beforeEach(() => {
    vi.clearAllMocks();

    // Default mock untuk getNotesState
    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: { documents: [], activePath: null, content: null, error: null },
    });

    // Default mock untuk pickNotesFile
    vi.spyOn(commands, "pickNotesFile").mockResolvedValue({
      status: "ok",
      data: null,
    });

    // Default mock untuk setActiveNotesFile & closeNotesFile
    vi.spyOn(commands, "setActiveNotesFile").mockResolvedValue({
      status: "ok",
      data: null,
    });
    vi.spyOn(commands, "closeNotesFile").mockResolvedValue({
      status: "ok",
      data: null,
    });

    // Default mock untuk events
    vi.spyOn(events.notesUpdate, "listen").mockResolvedValue(vi.fn());
    vi.spyOn(events.notesError, "listen").mockResolvedValue(vi.fn());
  });

  it("1. initial mount calls getNotesState and renders restored markdown content", async () => {
    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: {
        documents: [{ path: "C:\\notes.md", title: "notes.md", content: "# Heading 1\n* List item restored" }],
        activePath: "C:\\notes.md",
        content: "# Heading 1\n* List item restored",
        error: null,
      },
    });

    render(<NotesPanel />);

    await waitFor(() => {
      expect(commands.getNotesState).toHaveBeenCalledTimes(1);
    });

    expect(await screen.findByRole("heading", { level: 1, name: "Heading 1" })).toBeInTheDocument();
    expect(screen.getByText("List item restored")).toBeInTheDocument();
  });

  it("2. initial mount renders persistent error banner when getNotesState returns error", async () => {
    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: {
        documents: [],
        activePath: null,
        content: null,
        error: "File notes sebelumnya 'C:\\notes.md' tidak ditemukan.",
      },
    });

    render(<NotesPanel />);

    const alert = await screen.findByRole("alert");
    expect(alert).toBeInTheDocument();
    expect(alert).toHaveTextContent("File notes sebelumnya 'C:\\notes.md' tidak ditemukan.");
  });

  it("3. initial mount renders empty state placeholder when both content and error are null", async () => {
    render(<NotesPanel />);

    await waitFor(() => {
      expect(commands.getNotesState).toHaveBeenCalledTimes(1);
    });

    expect(screen.getByText("Belum ada file notes yang dipilih")).toBeInTheDocument();
  });

  it("4. pickNotesFile success with path does not show error banner", async () => {
    vi.spyOn(commands, "pickNotesFile").mockResolvedValue({
      status: "ok",
      data: { path: "C:\\docs\\notes.md", title: "notes.md", content: "# Notes" },
    });

    render(<NotesPanel />);
    await screen.findByText("Belum ada file notes yang dipilih");

    const pickButton = screen.getByRole("button", { name: /Pilih File/i });
    fireEvent.click(pickButton);

    await waitFor(() => {
      expect(commands.pickNotesFile).toHaveBeenCalledTimes(1);
    });

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("5. pickNotesFile success with null (cancel) is a clean no-op", async () => {
    vi.spyOn(commands, "pickNotesFile").mockResolvedValue({
      status: "ok",
      data: null,
    });

    render(<NotesPanel />);
    await screen.findByText("Belum ada file notes yang dipilih");

    const pickButton = screen.getByRole("button", { name: /Pilih File/i });
    fireEvent.click(pickButton);

    await waitFor(() => {
      expect(commands.pickNotesFile).toHaveBeenCalledTimes(1);
    });

    expect(screen.getByText("Belum ada file notes yang dipilih")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("6. pickNotesFile command error displays error message in banner", async () => {
    vi.spyOn(commands, "pickNotesFile").mockResolvedValue({
      status: "error",
      error: "Gagal membuka dialog file OS",
    });

    render(<NotesPanel />);
    await screen.findByText("Belum ada file notes yang dipilih");

    const pickButton = screen.getByRole("button", { name: /Pilih File/i });
    fireEvent.click(pickButton);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Gagal membuka dialog file OS");
  });

  it("7. notesUpdate event renders markdown content and clears existing error banner", async () => {
    let updateCallback: ((event: { payload: { path: string; content: string } }) => void) | null = null;
    vi.spyOn(events.notesUpdate, "listen").mockImplementation(async (cb) => {
      updateCallback = cb as unknown as (event: { payload: { path: string; content: string } }) => void;
      return vi.fn();
    });

    // Awal: ada error dari getNotesState
    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: { documents: [], activePath: null, content: null, error: "File lama error" },
    });

    render(<NotesPanel />);
    expect(await screen.findByRole("alert")).toHaveTextContent("File lama error");

    // Simulasi event notesUpdate masuk
    expect(updateCallback).not.toBeNull();
    act(() => {
      updateCallback!({ payload: { path: "C:\\notes.md", content: "# Updated Title\nParagraf baru." } });
    });

    // Assert: error banner hilang dan markdown baru ter-render
    await waitFor(() => {
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });
    expect(screen.getByRole("heading", { level: 1, name: "Updated Title" })).toBeInTheDocument();
    expect(screen.getByText("Paragraf baru.")).toBeInTheDocument();
  });

  it("8. notesError event displays persistent error banner", async () => {
    let errorCallback: ((event: { payload: { message: string } }) => void) | null = null;
    vi.spyOn(events.notesError, "listen").mockImplementation(async (cb) => {
      errorCallback = cb as unknown as (event: { payload: { message: string } }) => void;
      return vi.fn();
    });

    render(<NotesPanel />);
    await screen.findByText("Belum ada file notes yang dipilih");

    // Simulasi runtime error event
    expect(errorCallback).not.toBeNull();
    act(() => {
      errorCallback!({ payload: { message: "File notes telah dihapus dari disk." } });
    });

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("File notes telah dihapus dari disk.");
  });

  it("9. toggle between File Mode and Scratchpad preserves scratchpad input text", async () => {
    render(<NotesPanel />);
    await screen.findByText("Belum ada file notes yang dipilih");

    // Switch ke Scratchpad
    const scratchpadTabButton = screen.getByRole("button", { name: /Scratchpad/i });
    fireEvent.click(scratchpadTabButton);

    // Ketik catatan ad-hoc di textarea
    const textarea = screen.getByPlaceholderText(/Tulis catatan ad-hoc di sini/i);
    fireEvent.change(textarea, { target: { value: "Catatan temporary meeting 123" } });
    expect(textarea).toHaveValue("Catatan temporary meeting 123");

    // Switch kembali ke File Mode
    const fileModeTabButton = screen.getByRole("button", { name: /File Mode/i });
    fireEvent.click(fileModeTabButton);
    expect(screen.getByText("Belum ada file notes yang dipilih")).toBeInTheDocument();

    // Switch lagi ke Scratchpad -> input teks tetap tersimpan di state lokal
    fireEvent.click(screen.getByRole("button", { name: /Scratchpad/i }));
    const restoredTextarea = screen.getByPlaceholderText(/Tulis catatan ad-hoc di sini/i);
    expect(restoredTextarea).toHaveValue("Catatan temporary meeting 123");
  });

  it("10. unmount cleans up notesUpdate and notesError event listeners", async () => {
    const unlistenUpdateMock = vi.fn();
    const unlistenErrorMock = vi.fn();

    vi.spyOn(events.notesUpdate, "listen").mockResolvedValue(unlistenUpdateMock);
    vi.spyOn(events.notesError, "listen").mockResolvedValue(unlistenErrorMock);

    const { unmount } = render(<NotesPanel />);
    await screen.findByText("Belum ada file notes yang dipilih");

    await waitFor(() => {
      expect(events.notesUpdate.listen).toHaveBeenCalled();
      expect(events.notesError.listen).toHaveBeenCalled();
    });

    unmount();

    expect(unlistenUpdateMock).toHaveBeenCalled();
    expect(unlistenErrorMock).toHaveBeenCalled();
  });

  it("11. switching tabs updates active document and invokes setActiveNotesFile", async () => {
    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: {
        documents: [
          { path: "/notes/doc1.md", title: "doc1.md", content: "# Content Doc 1" },
          { path: "/notes/doc2.md", title: "doc2.md", content: "# Content Doc 2" },
        ],
        activePath: "/notes/doc1.md",
        content: "# Content Doc 1",
        error: null,
      },
    });

    render(<NotesPanel />);
    expect(await screen.findByRole("heading", { level: 1, name: "Content Doc 1" })).toBeInTheDocument();

    // Tab bar harus memiliki 2 tab
    const tab1 = screen.getByRole("tab", { name: /doc1.md/i });
    const tab2 = screen.getByRole("tab", { name: /doc2.md/i });
    expect(tab1).toHaveAttribute("aria-selected", "true");
    expect(tab2).toHaveAttribute("aria-selected", "false");

    // Klik tab kedua
    fireEvent.click(tab2);

    expect(commands.setActiveNotesFile).toHaveBeenCalledWith("/notes/doc2.md");
    expect(tab2).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("heading", { level: 1, name: "Content Doc 2" })).toBeInTheDocument();
  });

  it("12. closing active tab invokes closeNotesFile and switches active to sibling tab", async () => {
    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: {
        documents: [
          { path: "/notes/doc1.md", title: "doc1.md", content: "# Content Doc 1" },
          { path: "/notes/doc2.md", title: "doc2.md", content: "# Content Doc 2" },
        ],
        activePath: "/notes/doc1.md",
        content: "# Content Doc 1",
        error: null,
      },
    });

    render(<NotesPanel />);
    expect(await screen.findByRole("heading", { level: 1, name: "Content Doc 1" })).toBeInTheDocument();

    // Tutup tab pertama (yang sedang aktif)
    const closeTab1Btn = screen.getByRole("button", { name: "Tutup tab doc1.md" });
    fireEvent.click(closeTab1Btn);

    expect(commands.closeNotesFile).toHaveBeenCalledWith("/notes/doc1.md");

    // doc2.md menjadi tab aktif yang baru
    await waitFor(() => {
      expect(screen.queryByRole("tab", { name: /doc1.md/i })).not.toBeInTheDocument();
    });
    expect(screen.getByRole("tab", { name: /doc2.md/i })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("heading", { level: 1, name: "Content Doc 2" })).toBeInTheDocument();
  });

  it("13. picking a new file adds tab to document list and activates it", async () => {
    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: {
        documents: [{ path: "/notes/doc1.md", title: "doc1.md", content: "# Doc 1" }],
        activePath: "/notes/doc1.md",
        content: "# Doc 1",
        error: null,
      },
    });

    vi.spyOn(commands, "pickNotesFile").mockResolvedValue({
      status: "ok",
      data: { path: "/notes/doc3.md", title: "doc3.md", content: "# Doc 3 Newly Added" },
    });

    render(<NotesPanel />);
    expect(await screen.findByRole("tab", { name: /doc1.md/i })).toBeInTheDocument();

    // Klik tombol '+' pada tab bar
    const newTabBtn = screen.getByRole("button", { name: "Buka dokumen baru" });
    fireEvent.click(newTabBtn);

    await waitFor(() => {
      expect(commands.pickNotesFile).toHaveBeenCalledTimes(1);
    });

    // Tab baru doc3.md muncul dan aktif
    expect(await screen.findByRole("tab", { name: /doc3.md/i })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("heading", { level: 1, name: "Doc 3 Newly Added" })).toBeInTheDocument();
  });

  it("14. background notesUpdate for non-active tab updates its cached content without switching active tab", async () => {
    let updateCallback: ((event: { payload: { path: string; content: string } }) => void) | null = null;
    vi.spyOn(events.notesUpdate, "listen").mockImplementation(async (cb) => {
      updateCallback = cb as unknown as (event: { payload: { path: string; content: string } }) => void;
      return vi.fn();
    });

    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: {
        documents: [
          { path: "/notes/doc1.md", title: "doc1.md", content: "# Doc 1 Original" },
          { path: "/notes/doc2.md", title: "doc2.md", content: "# Doc 2 Original" },
        ],
        activePath: "/notes/doc1.md",
        content: "# Doc 1 Original",
        error: null,
      },
    });

    render(<NotesPanel />);
    expect(await screen.findByRole("heading", { level: 1, name: "Doc 1 Original" })).toBeInTheDocument();

    // Trigger update untuk doc2.md (yang sedang di background)
    act(() => {
      updateCallback!({
        payload: { path: "/notes/doc2.md", content: "# Doc 2 Modified in Background" },
      });
    });

    // Tab aktif tetap doc1.md
    expect(screen.getByRole("heading", { level: 1, name: "Doc 1 Original" })).toBeInTheDocument();

    // Sekarang beralih ke doc2.md
    const tab2 = screen.getByRole("tab", { name: /doc2.md/i });
    fireEvent.click(tab2);

    // Konten yang ter-render adalah konten yang sudah di-update di background
    expect(screen.getByRole("heading", { level: 1, name: "Doc 2 Modified in Background" })).toBeInTheDocument();
  });
});
