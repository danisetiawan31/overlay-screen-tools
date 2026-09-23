import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, act } from "@testing-library/react";
import React from "react";
import ReactMarkdown from "react-markdown";
import mermaid from "mermaid";
import {
  MermaidRenderer,
  extractTextContent,
  markdownComponents,
  copyTextToClipboard,
} from "./MermaidRenderer";
import { commands } from "../bindings";

vi.mock("mermaid", () => {
  return {
    default: {
      initialize: vi.fn(),
      render: vi.fn(),
    },
  };
});

describe("MermaidRenderer Component", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("1. renders SVG output when mermaid.render succeeds", async () => {
    const mockSvg = '<svg data-testid="svg-mock"><g><text>Diagram</text></g></svg>';
    vi.mocked(mermaid.render).mockResolvedValue({
      svg: mockSvg,
      bindFunctions: undefined,
      diagramType: "flowchart",
    });

    render(<MermaidRenderer chart="graph TD\nA-->B" />);

    expect(screen.getByTestId("mermaid-loading")).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByTestId("mermaid-diagram")).toBeInTheDocument();
    });

    expect(screen.getByTestId("mermaid-diagram").innerHTML).toContain(mockSvg);
    expect(mermaid.render).toHaveBeenCalledTimes(1);
  });

  it("2. renders fallback error UI when mermaid.render throws an error", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.mocked(mermaid.render).mockRejectedValue(new Error("Parse error"));

    render(<MermaidRenderer chart="graph INVALID" />);

    await waitFor(() => {
      expect(screen.getByTestId("mermaid-error")).toBeInTheDocument();
    });

    expect(screen.getByText(/sintaks tidak valid/i)).toBeInTheDocument();
    expect(screen.getByText("graph INVALID")).toBeInTheDocument();
  });

  it("3. handles empty chart string gracefully without rendering", async () => {
    render(<MermaidRenderer chart="   " />);

    // Empty chart should not call mermaid.render
    expect(mermaid.render).not.toHaveBeenCalled();
  });

  it("4. extractTextContent handles strings, numbers, arrays, and nested elements", () => {
    expect(extractTextContent("simple text")).toBe("simple text");
    expect(extractTextContent(42)).toBe("42");
    expect(extractTextContent(["part1", " ", "part2"])).toBe("part1 part2");
    expect(
      extractTextContent(
        React.createElement("span", {}, [
          React.createElement("strong", { key: "k1" }, "bold"),
          " normal",
        ])
      )
    ).toBe("bold normal");
    expect(extractTextContent(null)).toBe("");
  });

  it("5. markdownComponents.code intercepts language-mermaid", async () => {
    vi.mocked(mermaid.render).mockResolvedValue({
      svg: "<svg>test</svg>",
      bindFunctions: undefined,
      diagramType: "flowchart",
    });

    // Test code component for mermaid
    const CodeComp = markdownComponents.code;
    const { container } = render(
      React.createElement(CodeComp, {
        className: "language-mermaid",
        children: "graph LR\n  A --> B",
      })
    );

    expect(container.querySelector("[data-testid='mermaid-loading']")).toBeInTheDocument();

    await waitFor(() => {
      expect(container.querySelector("[data-testid='mermaid-diagram']")).toBeInTheDocument();
    });
  });

  it("6. markdownComponents.code renders standard code tag for other languages", () => {
    const CodeComp = markdownComponents.code;
    render(
      React.createElement(CodeComp, {
        className: "language-typescript",
        children: "const x = 1;",
      })
    );

    const codeEl = screen.getByText("const x = 1;");
    expect(codeEl.tagName).toBe("CODE");
    expect(codeEl).toHaveClass("language-typescript");
  });

  it("7. markdownComponents.pre wraps code block with language label and copy button", async () => {
    const PreComp = markdownComponents.pre;
    const writeTextSpy = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { clipboard: { writeText: writeTextSpy } });

    render(
      React.createElement(
        PreComp,
        {},
        React.createElement("code", { className: "language-rust" }, 'println!("Hello!");')
      )
    );

    expect(screen.getByText("rust")).toBeInTheDocument();
    const copyBtn = screen.getByRole("button", { name: "Salin kode" });
    expect(copyBtn).toBeInTheDocument();

    await act(async () => {
      copyBtn.click();
    });
    expect(writeTextSpy).toHaveBeenCalledWith('println!("Hello!");');

    vi.unstubAllGlobals();

  });

  it("8. markdownComponents.code renders inline code badge when not a language code block", () => {
    const CodeComp = markdownComponents.code;
    render(
      React.createElement(CodeComp, {
        children: "npm install",
      })
    );

    const inlineEl = screen.getByText("npm install");
    expect(inlineEl.tagName).toBe("CODE");
    expect(inlineEl.className).toContain("text-amber-300");
  });

  it("9. copyTextToClipboard prioritizes commands.copyToClipboard", async () => {
    const copySpy = vi.spyOn(commands, "copyToClipboard").mockResolvedValue({
      status: "ok",
      data: null,
    });
    const writeTextSpy = vi.fn();
    vi.stubGlobal("navigator", { clipboard: { writeText: writeTextSpy } });

    const ok = await copyTextToClipboard("const a = 10;");
    expect(ok).toBe(true);
    expect(copySpy).toHaveBeenCalledWith({ text: "const a = 10;" });
    expect(writeTextSpy).not.toHaveBeenCalled();

    vi.unstubAllGlobals();
  });

  it("10. markdownComponents.blockquote renders GitHub Alert [!NOTE] as high-contrast callout card", () => {
    const BlockquoteComp = markdownComponents.blockquote;
    render(
      React.createElement(BlockquoteComp, {
        children: [
          React.createElement("p", { key: "p1" }, "[!NOTE]\nIni pertanyaan wawancara penting"),
          React.createElement("p", { key: "p2" }, "Paragraf kedua"),
        ],
      })
    );

    const alertRegion = screen.getByRole("region", { name: /note alert/i });
    expect(alertRegion).toBeInTheDocument();
    expect(alertRegion.className).toContain("border-l-sky-400");
    expect(screen.getByText("Note")).toBeInTheDocument();
    expect(screen.getByText(/Ini pertanyaan wawancara penting/)).toBeInTheDocument();
    expect(screen.queryByText(/\[!NOTE\]/)).not.toBeInTheDocument();
  });

  it("11. markdownComponents.blockquote renders GitHub Alert [!TIP] with emerald styling", () => {
    const BlockquoteComp = markdownComponents.blockquote;
    render(
      React.createElement(BlockquoteComp, {
        children: [
          React.createElement("p", { key: "p1" }, "[!TIP] Gunakan STAR method"),
        ],
      })
    );

    const alertRegion = screen.getByRole("region", { name: /tip alert/i });
    expect(alertRegion).toBeInTheDocument();
    expect(alertRegion.className).toContain("border-l-emerald-400");
    expect(screen.getByText("Tip")).toBeInTheDocument();
    expect(screen.getByText(/Gunakan STAR method/)).toBeInTheDocument();
  });

  it("12. markdownComponents.blockquote renders standard blockquote when no alert tag is present", () => {
    const BlockquoteComp = markdownComponents.blockquote;
    const { container } = render(
      React.createElement(BlockquoteComp, {
        children: React.createElement("p", {}, "Ini kutipan biasa tanpa alert"),
      })
    );

    expect(screen.queryByRole("region")).not.toBeInTheDocument();
    const bq = container.querySelector("blockquote");
    expect(bq).toBeInTheDocument();
    expect(bq?.textContent).toContain("Ini kutipan biasa tanpa alert");
  });

  it("13. renders real ReactMarkdown document with > [!NOTE] callout and regular quote", () => {
    const markdown = [
      "> [!NOTE]",
      "> ### 🎙️ PERTANYAAN PEWAWANCARA:",
      '> *"Kenapa memilih magang, bukan langsung melamar kerja full-time?"*',
      "",
      "#### Skrip Jawaban:",
      '> "Sebagai fresh graduate S1 Sistem Informasi..."',
    ].join("\n");

    const { container } = render(
      React.createElement(ReactMarkdown, { components: markdownComponents, children: markdown })
    );

    // Callout alert container
    const alertRegion = screen.getByRole("region", { name: /note alert/i });
    expect(alertRegion).toBeInTheDocument();
    expect(alertRegion.className).toContain("border-l-sky-400");
    expect(screen.getByText("Note")).toBeInTheDocument();
    expect(screen.getByText(/PERTANYAAN PEWAWANCARA/)).toBeInTheDocument();
    expect(screen.queryByText(/\[!NOTE\]/)).not.toBeInTheDocument();

    // Regular quote is standard blockquote
    const bq = container.querySelector("blockquote");
    expect(bq).toBeInTheDocument();
    expect(bq?.textContent).toContain("Sebagai fresh graduate S1");
  });
});

