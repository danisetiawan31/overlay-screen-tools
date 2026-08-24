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
      data: { content: null, error: null },
    });

    // Default mock untuk pickNotesFile
    vi.spyOn(commands, "pickNotesFile").mockResolvedValue({
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
      data: { content: "# Heading 1\n* List item restored", error: null },
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
      data: { content: null, error: "File notes sebelumnya 'C:\\notes.md' tidak ditemukan." },
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
      data: { path: "C:\\docs\\notes.md" },
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
    let updateCallback: ((event: { payload: { content: string } }) => void) | null = null;
    vi.spyOn(events.notesUpdate, "listen").mockImplementation(async (cb) => {
      updateCallback = cb as unknown as (event: { payload: { content: string } }) => void;
      return vi.fn();
    });

    // Awal: ada error dari getNotesState
    vi.spyOn(commands, "getNotesState").mockResolvedValue({
      status: "ok",
      data: { content: null, error: "File lama error" },
    });

    render(<NotesPanel />);
    expect(await screen.findByRole("alert")).toHaveTextContent("File lama error");

    // Simulasi event notesUpdate masuk
    expect(updateCallback).not.toBeNull();
    act(() => {
      updateCallback!({ payload: { content: "# Updated Title\nParagraf baru." } });
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
});
