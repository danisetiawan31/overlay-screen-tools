import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { FindBar } from "./FindBar";

describe("FindBar Component", () => {
  it("renders input field and automatically focuses it on mount", () => {
    render(
      <FindBar
        query=""
        onQueryChange={vi.fn()}
        currentIndex={0}
        totalMatches={0}
        onNext={vi.fn()}
        onPrev={vi.fn()}
        onClose={vi.fn()}
      />
    );

    const input = screen.getByPlaceholderText("Cari di dokumen...");
    expect(input).toBeInTheDocument();
    expect(input).toHaveFocus();
  });

  it("calls onQueryChange when user types in input", () => {
    const handleQueryChange = vi.fn();
    render(
      <FindBar
        query=""
        onQueryChange={handleQueryChange}
        currentIndex={0}
        totalMatches={0}
        onNext={vi.fn()}
        onPrev={vi.fn()}
        onClose={vi.fn()}
      />
    );

    const input = screen.getByPlaceholderText("Cari di dokumen...");
    fireEvent.change(input, { target: { value: "magang" } });
    expect(handleQueryChange).toHaveBeenCalledWith("magang");
  });

  it("shows 0 / 0 when query is non-empty but totalMatches is 0", () => {
    render(
      <FindBar
        query="tidak_ada"
        onQueryChange={vi.fn()}
        currentIndex={0}
        totalMatches={0}
        onNext={vi.fn()}
        onPrev={vi.fn()}
        onClose={vi.fn()}
      />
    );

    const counter = screen.getByTestId("find-counter");
    expect(counter).toHaveTextContent("0 / 0");
    expect(counter).toHaveClass("text-rose-400");
  });

  it("shows currentIndex + 1 / totalMatches when matches exist", () => {
    render(
      <FindBar
        query="react"
        onQueryChange={vi.fn()}
        currentIndex={2}
        totalMatches={7}
        onNext={vi.fn()}
        onPrev={vi.fn()}
        onClose={vi.fn()}
      />
    );

    const counter = screen.getByTestId("find-counter");
    expect(counter).toHaveTextContent("3 / 7");
  });

  it("triggers onNext on Enter and onPrev on Shift+Enter", () => {
    const handleNext = vi.fn();
    const handlePrev = vi.fn();

    render(
      <FindBar
        query="test"
        onQueryChange={vi.fn()}
        currentIndex={0}
        totalMatches={3}
        onNext={handleNext}
        onPrev={handlePrev}
        onClose={vi.fn()}
      />
    );

    const input = screen.getByPlaceholderText("Cari di dokumen...");

    // Enter -> onNext
    fireEvent.keyDown(input, { key: "Enter", shiftKey: false });
    expect(handleNext).toHaveBeenCalledTimes(1);

    // Shift + Enter -> onPrev
    fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
    expect(handlePrev).toHaveBeenCalledTimes(1);
  });

  it("triggers onClose on Escape key", () => {
    const handleClose = vi.fn();

    render(
      <FindBar
        query="test"
        onQueryChange={vi.fn()}
        currentIndex={0}
        totalMatches={3}
        onNext={vi.fn()}
        onPrev={vi.fn()}
        onClose={handleClose}
      />
    );

    const input = screen.getByPlaceholderText("Cari di dokumen...");
    fireEvent.keyDown(input, { key: "Escape" });
    expect(handleClose).toHaveBeenCalledTimes(1);
  });

  it("triggers button clicks and disables navigation when no matches", () => {
    const handleNext = vi.fn();
    const handlePrev = vi.fn();
    const handleClose = vi.fn();

    const { rerender } = render(
      <FindBar
        query="test"
        onQueryChange={vi.fn()}
        currentIndex={0}
        totalMatches={0}
        onNext={handleNext}
        onPrev={handlePrev}
        onClose={handleClose}
      />
    );

    const prevBtn = screen.getByRole("button", { name: "Hasil sebelumnya" });
    const nextBtn = screen.getByRole("button", { name: "Hasil berikutnya" });
    const closeBtn = screen.getByRole("button", { name: "Tutup pencarian" });

    expect(prevBtn).toBeDisabled();
    expect(nextBtn).toBeDisabled();

    // Re-render with matches
    rerender(
      <FindBar
        query="test"
        onQueryChange={vi.fn()}
        currentIndex={0}
        totalMatches={3}
        onNext={handleNext}
        onPrev={handlePrev}
        onClose={handleClose}
      />
    );

    expect(prevBtn).not.toBeDisabled();
    expect(nextBtn).not.toBeDisabled();

    fireEvent.click(nextBtn);
    expect(handleNext).toHaveBeenCalledTimes(1);

    fireEvent.click(prevBtn);
    expect(handlePrev).toHaveBeenCalledTimes(1);

    fireEvent.click(closeBtn);
    expect(handleClose).toHaveBeenCalledTimes(1);
  });
});
