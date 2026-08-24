import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { TabNav } from "./TabNav";

describe("TabNav Component", () => {
  it("renders Notes and Live Q&A tabs correctly", () => {
    const handleTabChange = vi.fn();
    render(
      <TabNav activeTab="notes" onTabChange={handleTabChange} qaAvailable={true} />
    );

    expect(screen.getByRole("tab", { name: /^notes$/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /^live q&a$/i })).toBeInTheDocument();
  });

  it("calls onTabChange with 'qa' when clicking Live Q&A tab while qaAvailable is true", () => {
    const handleTabChange = vi.fn();
    render(
      <TabNav activeTab="notes" onTabChange={handleTabChange} qaAvailable={true} />
    );

    const qaTab = screen.getByRole("tab", { name: /^live q&a$/i });
    fireEvent.click(qaTab);

    expect(handleTabChange).toHaveBeenCalledWith("qa");
  });

  it("calls onTabChange with 'notes' when clicking Notes tab", () => {
    const handleTabChange = vi.fn();
    render(
      <TabNav activeTab="qa" onTabChange={handleTabChange} qaAvailable={true} />
    );

    const notesTab = screen.getByRole("tab", { name: /^notes$/i });
    fireEvent.click(notesTab);

    expect(handleTabChange).toHaveBeenCalledWith("notes");
  });

  it("disables Live Q&A tab and ignores clicks when qaAvailable is false", () => {
    const handleTabChange = vi.fn();
    render(
      <TabNav activeTab="notes" onTabChange={handleTabChange} qaAvailable={false} />
    );

    const qaTab = screen.getByRole("tab", { name: /live q&a/i });
    expect(qaTab).toBeDisabled();
    expect(qaTab).toHaveAttribute("title", "F8 tidak tersedia (hotkey gagal didaftarkan)");
    expect(screen.getByText("Off")).toBeInTheDocument();

    fireEvent.click(qaTab);
    expect(handleTabChange).not.toHaveBeenCalled();
  });
});
