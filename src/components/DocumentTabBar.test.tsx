import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { DocumentTabBar } from "./DocumentTabBar";
import type { NoteDocument } from "../bindings";

describe("DocumentTabBar Component", () => {
  const mockDocs: NoteDocument[] = [
    { path: "/notes/doc1.md", title: "doc1.md", content: "Doc 1 content" },
    { path: "/notes/doc2.md", title: "doc2.md", content: "Doc 2 content" },
  ];

  it("renders all open document tabs with titles", () => {
    render(
      <DocumentTabBar
        documents={mockDocs}
        activePath="/notes/doc1.md"
        onSelectTab={vi.fn()}
        onCloseTab={vi.fn()}
        onNewTab={vi.fn()}
      />
    );

    expect(screen.getByText("doc1.md")).toBeInTheDocument();
    expect(screen.getByText("doc2.md")).toBeInTheDocument();
  });

  it("marks active tab correctly with aria-selected", () => {
    render(
      <DocumentTabBar
        documents={mockDocs}
        activePath="/notes/doc2.md"
        onSelectTab={vi.fn()}
        onCloseTab={vi.fn()}
        onNewTab={vi.fn()}
      />
    );

    const tabs = screen.getAllByRole("tab");
    expect(tabs[0]).toHaveAttribute("aria-selected", "false");
    expect(tabs[1]).toHaveAttribute("aria-selected", "true");
  });

  it("calls onSelectTab when clicking an inactive tab", () => {
    const handleSelect = vi.fn();
    render(
      <DocumentTabBar
        documents={mockDocs}
        activePath="/notes/doc1.md"
        onSelectTab={handleSelect}
        onCloseTab={vi.fn()}
        onNewTab={vi.fn()}
      />
    );

    fireEvent.click(screen.getByText("doc2.md"));
    expect(handleSelect).toHaveBeenCalledWith("/notes/doc2.md");
  });

  it("calls onCloseTab when clicking close button without selecting tab", () => {
    const handleSelect = vi.fn();
    const handleClose = vi.fn();
    render(
      <DocumentTabBar
        documents={mockDocs}
        activePath="/notes/doc1.md"
        onSelectTab={handleSelect}
        onCloseTab={handleClose}
        onNewTab={vi.fn()}
      />
    );

    const closeBtn = screen.getByRole("button", { name: /tutup tab doc1\.md/i });
    fireEvent.click(closeBtn);

    expect(handleClose).toHaveBeenCalledWith("/notes/doc1.md");
    expect(handleSelect).not.toHaveBeenCalled();
  });

  it("calls onNewTab when clicking the plus button", () => {
    const handleNew = vi.fn();
    render(
      <DocumentTabBar
        documents={mockDocs}
        activePath="/notes/doc1.md"
        onSelectTab={vi.fn()}
        onCloseTab={vi.fn()}
        onNewTab={handleNew}
      />
    );

    const newBtn = screen.getByRole("button", { name: /buka dokumen baru/i });
    fireEvent.click(newBtn);
    expect(handleNew).toHaveBeenCalledTimes(1);
  });
});
