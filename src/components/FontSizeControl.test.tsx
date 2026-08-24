import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { FontSizeControl, MIN_FONT_SIZE, MAX_FONT_SIZE } from "./FontSizeControl";

describe("FontSizeControl", () => {
  it("renders current font size label correctly", () => {
    const handleUpdate = vi.fn();
    render(<FontSizeControl fontSize={14} onUpdateFontSize={handleUpdate} />);

    expect(screen.getByText("14px")).toBeInTheDocument();
  });

  it("disables decrease button when at lower boundary (MIN_FONT_SIZE)", () => {
    const handleUpdate = vi.fn();
    render(<FontSizeControl fontSize={MIN_FONT_SIZE} onUpdateFontSize={handleUpdate} />);

    const decBtn = screen.getByRole("button", { name: /decrease font size/i });
    const incBtn = screen.getByRole("button", { name: /increase font size/i });

    expect(decBtn).toBeDisabled();
    expect(incBtn).not.toBeDisabled();
  });

  it("disables increase button when at upper boundary (MAX_FONT_SIZE)", () => {
    const handleUpdate = vi.fn();
    render(<FontSizeControl fontSize={MAX_FONT_SIZE} onUpdateFontSize={handleUpdate} />);

    const decBtn = screen.getByRole("button", { name: /decrease font size/i });
    const incBtn = screen.getByRole("button", { name: /increase font size/i });

    expect(decBtn).not.toBeDisabled();
    expect(incBtn).toBeDisabled();
  });

  it("calls onUpdateFontSize with decremented size on decrease click", () => {
    const handleUpdate = vi.fn();
    render(<FontSizeControl fontSize={14} onUpdateFontSize={handleUpdate} />);

    const decBtn = screen.getByRole("button", { name: /decrease font size/i });
    fireEvent.click(decBtn);

    expect(handleUpdate).toHaveBeenCalledTimes(1);
    expect(handleUpdate).toHaveBeenCalledWith(13);
  });

  it("calls onUpdateFontSize with incremented size on increase click", () => {
    const handleUpdate = vi.fn();
    render(<FontSizeControl fontSize={14} onUpdateFontSize={handleUpdate} />);

    const incBtn = screen.getByRole("button", { name: /increase font size/i });
    fireEvent.click(incBtn);

    expect(handleUpdate).toHaveBeenCalledTimes(1);
    expect(handleUpdate).toHaveBeenCalledWith(15);
  });

  it("disables both buttons when disabled prop is true", () => {
    const handleUpdate = vi.fn();
    render(<FontSizeControl fontSize={14} onUpdateFontSize={handleUpdate} disabled={true} />);

    expect(screen.getByRole("button", { name: /decrease font size/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /increase font size/i })).toBeDisabled();
  });
});
