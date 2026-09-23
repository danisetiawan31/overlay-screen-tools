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
    vi.spyOn(events.notesCycleTab, "listen").mockResolvedValue(vi.fn());
    vi.spyOn(events.notesToggleFind, "listen").mockResolvedValue(vi.fn());
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

  it("15. Ctrl + Shift + ArrowRight navigates to next tab and wraps around", async () => {
    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: {
        documents: [
          { path: "/notes/doc1.md", title: "doc1.md", content: "# Doc 1" },
          { path: "/notes/doc2.md", title: "doc2.md", content: "# Doc 2" },
          { path: "/notes/doc3.md", title: "doc3.md", content: "# Doc 3" },
        ],
        activePath: "/notes/doc1.md",
        content: "# Doc 1",
        error: null,
      },
    });

    render(<NotesPanel />);
    expect(await screen.findByRole("heading", { level: 1, name: "Doc 1" })).toBeInTheDocument();

    // 1st press: doc1 -> doc2
    fireEvent.keyDown(window, { key: "ArrowRight", ctrlKey: true, shiftKey: true });
    expect(commands.setActiveNotesFile).toHaveBeenCalledWith("/notes/doc2.md");
    expect(await screen.findByRole("heading", { level: 1, name: "Doc 2" })).toBeInTheDocument();

    // 2nd press: doc2 -> doc3
    fireEvent.keyDown(window, { key: "ArrowRight", ctrlKey: true, shiftKey: true });
    expect(commands.setActiveNotesFile).toHaveBeenCalledWith("/notes/doc3.md");
    expect(await screen.findByRole("heading", { level: 1, name: "Doc 3" })).toBeInTheDocument();

    // 3rd press: doc3 -> wraps around to doc1
    fireEvent.keyDown(window, { key: "ArrowRight", ctrlKey: true, shiftKey: true });
    expect(commands.setActiveNotesFile).toHaveBeenCalledWith("/notes/doc1.md");
    expect(await screen.findByRole("heading", { level: 1, name: "Doc 1" })).toBeInTheDocument();
  });

  it("16. Ctrl + Shift + ArrowLeft navigates to previous tab and wraps around", async () => {
    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: {
        documents: [
          { path: "/notes/doc1.md", title: "doc1.md", content: "# Doc 1" },
          { path: "/notes/doc2.md", title: "doc2.md", content: "# Doc 2" },
        ],
        activePath: "/notes/doc1.md",
        content: "# Doc 1",
        error: null,
      },
    });

    render(<NotesPanel />);
    expect(await screen.findByRole("heading", { level: 1, name: "Doc 1" })).toBeInTheDocument();

    // 1st press left: doc1 -> wraps around to doc2
    fireEvent.keyDown(window, { key: "ArrowLeft", ctrlKey: true, shiftKey: true });
    expect(commands.setActiveNotesFile).toHaveBeenCalledWith("/notes/doc2.md");
    expect(await screen.findByRole("heading", { level: 1, name: "Doc 2" })).toBeInTheDocument();

    // 2nd press left: doc2 -> doc1
    fireEvent.keyDown(window, { key: "ArrowLeft", ctrlKey: true, shiftKey: true });
    expect(commands.setActiveNotesFile).toHaveBeenCalledWith("/notes/doc1.md");
    expect(await screen.findByRole("heading", { level: 1, name: "Doc 1" })).toBeInTheDocument();
  });

  it("17. does not trigger tab navigation if documents length <= 1", async () => {
    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: {
        documents: [{ path: "/notes/doc1.md", title: "doc1.md", content: "# Solo Doc" }],
        activePath: "/notes/doc1.md",
        content: "# Solo Doc",
        error: null,
      },
    });

    render(<NotesPanel />);
    expect(await screen.findByRole("heading", { level: 1, name: "Solo Doc" })).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "ArrowRight", ctrlKey: true, shiftKey: true });
    expect(commands.setActiveNotesFile).not.toHaveBeenCalled();
  });

  it("18. does not trigger tab navigation when focus is inside a textarea or input", async () => {
    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: {
        documents: [
          { path: "/notes/doc1.md", title: "doc1.md", content: "# Doc 1" },
          { path: "/notes/doc2.md", title: "doc2.md", content: "# Doc 2" },
        ],
        activePath: "/notes/doc1.md",
        content: "# Doc 1",
        error: null,
      },
    });

    render(<NotesPanel />);
    expect(await screen.findByRole("heading", { level: 1, name: "Doc 1" })).toBeInTheDocument();

    // Switch to scratchpad mode where textarea exists
    const scratchpadButton = screen.getByRole("button", { name: /scratchpad/i });
    fireEvent.click(scratchpadButton);

    const textarea = screen.getByPlaceholderText(/tulis catatan ad-hoc/i);
    fireEvent.keyDown(textarea, { key: "ArrowRight", ctrlKey: true, shiftKey: true });

    expect(commands.setActiveNotesFile).not.toHaveBeenCalled();
  });

  it("19. Ctrl + ArrowRight (without shift) and Ctrl + Tab also navigate document tabs", async () => {
    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: {
        documents: [
          { path: "/notes/doc1.md", title: "doc1.md", content: "# Doc 1" },
          { path: "/notes/doc2.md", title: "doc2.md", content: "# Doc 2" },
        ],
        activePath: null, // Fallback null test
        content: "# Doc 1",
        error: null,
      },
    });

    render(<NotesPanel />);
    expect(await screen.findByRole("heading", { level: 1, name: "Doc 1" })).toBeInTheDocument();

    // Ctrl + ArrowRight without shift
    fireEvent.keyDown(window, { key: "ArrowRight", ctrlKey: true, shiftKey: false });
    expect(commands.setActiveNotesFile).toHaveBeenCalledWith("/notes/doc2.md");

    // Ctrl + Tab
    fireEvent.keyDown(window, { key: "Tab", ctrlKey: true, shiftKey: false });
    expect(commands.setActiveNotesFile).toHaveBeenCalledWith("/notes/doc1.md");
  });

  it("cycles to next document tab when events.notesCycleTab (Ctrl+F10) fires", async () => {
    let cycleTabCallback: (() => void) | null = null;
    vi.spyOn(events.notesCycleTab, "listen").mockImplementation(async (cb) => {
      cycleTabCallback = cb as unknown as () => void;
      return vi.fn();
    });

    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: {
        documents: [
          { path: "/notes/doc1.md", title: "doc1.md", content: "# Doc 1" },
          { path: "/notes/doc2.md", title: "doc2.md", content: "# Doc 2" },
          { path: "/notes/doc3.md", title: "doc3.md", content: "# Doc 3" },
        ],
        activePath: "/notes/doc1.md",
        content: "# Doc 1",
        error: null,
      },
    });

    render(<NotesPanel />);
    expect(await screen.findByRole("heading", { level: 1, name: "Doc 1" })).toBeInTheDocument();

    // Trigger cycle 1 -> /notes/doc2.md
    act(() => {
      cycleTabCallback?.();
    });
    expect(commands.setActiveNotesFile).toHaveBeenCalledWith("/notes/doc2.md");

    // Trigger cycle 2 -> /notes/doc3.md
    act(() => {
      cycleTabCallback?.();
    });
    expect(commands.setActiveNotesFile).toHaveBeenCalledWith("/notes/doc3.md");
  });

  it("preserves individual scroll positions when switching between document tabs", async () => {
    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: {
        documents: [
          { path: "/notes/doc1.md", title: "doc1.md", content: "# Doc 1\n\nLong content 1" },
          { path: "/notes/doc2.md", title: "doc2.md", content: "# Doc 2\n\nLong content 2" },
        ],
        activePath: "/notes/doc1.md",
        content: "# Doc 1\n\nLong content 1",
        error: null,
      },
    });

    render(<NotesPanel />);
    expect(await screen.findByRole("heading", { level: 1, name: "Doc 1" })).toBeInTheDocument();

    const doc1Container = screen.getByTestId("doc-container-/notes/doc1.md");
    const doc2Container = screen.getByTestId("doc-container-/notes/doc2.md");

    expect(doc1Container).not.toHaveAttribute("hidden");
    expect(doc2Container).toHaveAttribute("hidden");

    // Scroll Doc 1 to 450px
    doc1Container.scrollTop = 450;
    fireEvent.scroll(doc1Container, { target: { scrollTop: 450 } });

    // Switch to Doc 2
    const tab2 = screen.getByRole("tab", { name: /doc2.md/i });
    fireEvent.click(tab2);

    expect(doc1Container).toHaveAttribute("hidden");
    expect(doc2Container).not.toHaveAttribute("hidden");

    // Scroll Doc 2 to 800px
    doc2Container.scrollTop = 800;
    fireEvent.scroll(doc2Container, { target: { scrollTop: 800 } });

    // Switch back to Doc 1
    const tab1 = screen.getByRole("tab", { name: /doc1.md/i });
    fireEvent.click(tab1);

    expect(doc1Container).not.toHaveAttribute("hidden");
    expect(doc2Container).toHaveAttribute("hidden");
    expect(doc1Container.scrollTop).toBe(450);

    // Switch back to Doc 2
    fireEvent.click(tab2);
    expect(doc2Container).not.toHaveAttribute("hidden");
    expect(doc2Container.scrollTop).toBe(800);
  });

  it("toggles FindBar on search button click and performs search", async () => {
    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: {
        documents: [
          { path: "/notes/doc1.md", title: "doc1.md", content: "# Magang Hub\n\nProgram magang sistem informasi magang" },
        ],
        activePath: "/notes/doc1.md",
        content: "# Magang Hub\n\nProgram magang sistem informasi magang",
        error: null,
      },
    });

    render(<NotesPanel />);
    expect(await screen.findByRole("heading", { level: 1, name: "Magang Hub" })).toBeInTheDocument();

    // Search bar awalnya tidak muncul
    expect(screen.queryByRole("search")).not.toBeInTheDocument();

    // Klik tombol cari 🔍
    const searchBtn = screen.getByRole("button", { name: "Cari di dokumen" });
    fireEvent.click(searchBtn);

    // Search bar muncul
    expect(screen.getByRole("search")).toBeInTheDocument();

    // Ketik query pencarian
    const searchInput = screen.getByPlaceholderText("Cari di dokumen...");
    fireEvent.change(searchInput, { target: { value: "magang" } });

    // Counter menampilkan 3 hasil (Magang Hub, magang, magang)
    const counter = await screen.findByTestId("find-counter");
    expect(counter).toHaveTextContent("1 / 3");

    // Tutup search bar
    const closeBtn = screen.getByRole("button", { name: "Tutup pencarian" });
    fireEvent.click(closeBtn);
    expect(screen.queryByRole("search")).not.toBeInTheDocument();
  });

  it("toggles FindBar when events.notesToggleFind (Ctrl+F11) fires", async () => {
    let toggleFindCallback: (() => void) | null = null;
    vi.spyOn(events.notesToggleFind, "listen").mockImplementation(async (cb) => {
      toggleFindCallback = cb as unknown as () => void;
      return vi.fn();
    });

    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: {
        documents: [
          { path: "/notes/doc1.md", title: "doc1.md", content: "# Dokumentasi" },
        ],
        activePath: "/notes/doc1.md",
        content: "# Dokumentasi",
        error: null,
      },
    });

    render(<NotesPanel />);
    expect(await screen.findByRole("heading", { level: 1, name: "Dokumentasi" })).toBeInTheDocument();

    expect(screen.queryByRole("search")).not.toBeInTheDocument();

    // Trigger Ctrl+F12 via callback
    act(() => {
      toggleFindCallback?.();
    });

    expect(screen.getByRole("search")).toBeInTheDocument();

    // Trigger Ctrl+F12 lagi untuk menutup
    act(() => {
      toggleFindCallback?.();
    });

    expect(screen.queryByRole("search")).not.toBeInTheDocument();
  });
});
