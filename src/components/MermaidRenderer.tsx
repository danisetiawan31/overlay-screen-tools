import React, { useEffect, useState } from "react";
import mermaid from "mermaid";

mermaid.initialize({
  startOnLoad: false,
  theme: "dark",
  securityLevel: "loose",
  suppressErrorRendering: true,
  themeVariables: {
    darkMode: true,
    background: "transparent",
    primaryColor: "#27272a",
    primaryTextColor: "#f4f4f5",
    primaryBorderColor: "#52525b",
    lineColor: "#a1a1aa",
    secondaryColor: "#18181b",
    tertiaryColor: "#09090b",
  },
});

export interface MermaidRendererProps {
  chart: string;
}

export const MermaidRenderer: React.FC<MermaidRendererProps> = ({ chart }) => {
  const [svgContent, setSvgContent] = useState<string>("");
  const [hasError, setHasError] = useState<boolean>(false);

  useEffect(() => {
    let isCancelled = false;

    async function renderDiagram() {
      const cleanChart = chart.trim();
      if (!cleanChart) {
        setSvgContent("");
        setHasError(false);
        return;
      }

      try {
        setHasError(false);
        const uniqueId = `mermaid-${Math.random().toString(36).substring(2, 9)}`;
        const { svg } = await mermaid.render(uniqueId, cleanChart);
        if (!isCancelled) {
          setSvgContent(svg);
        }
      } catch (err: unknown) {
        if (!isCancelled) {
          console.warn("[MERMAID] Gagal merender diagram:", err);
          setHasError(true);
        }
      }
    }

    renderDiagram();

    return () => {
      isCancelled = true;
    };
  }, [chart]);

  if (hasError) {
    return (
      <div
        data-testid="mermaid-error"
        className="my-3 rounded-lg border border-amber-800/60 bg-amber-950/30 p-3 text-xs"
      >
        <div className="mb-1.5 font-semibold text-amber-400">
          Diagram Mermaid (Sintaks tidak valid atau belum lengkap)
        </div>
        <pre className="overflow-x-auto text-zinc-300 font-mono bg-zinc-900/80 p-2 rounded">
          <code>{chart}</code>
        </pre>
      </div>
    );
  }

  if (!svgContent) {
    return (
      <div
        data-testid="mermaid-loading"
        className="my-3 flex items-center justify-center p-3 text-xs text-zinc-500 bg-zinc-900/40 rounded-lg border border-zinc-800/50 animate-pulse"
      >
        Memuat diagram Mermaid...
      </div>
    );
  }

  return (
    <div
      data-testid="mermaid-diagram"
      className="my-3 flex justify-center overflow-x-auto rounded-lg border border-zinc-800/80 bg-zinc-900/60 p-4 select-none [&_svg]:max-w-full [&_svg]:h-auto"
      dangerouslySetInnerHTML={{ __html: svgContent }}
    />
  );
};

export function extractTextContent(children: React.ReactNode): string {
  if (typeof children === "string") return children;
  if (typeof children === "number") return String(children);
  if (Array.isArray(children)) return children.map(extractTextContent).join("");
  if (
    React.isValidElement(children) &&
    children.props &&
    (children.props as { children?: React.ReactNode }).children
  ) {
    return extractTextContent((children.props as { children?: React.ReactNode }).children);
  }
  return "";
}

export const markdownComponents = {
  code(props: React.ComponentPropsWithoutRef<"code">) {
    const { children, className, ...rest } = props;
    const match = /language-(\w+)/.exec(className || "");
    if (match && match[1] === "mermaid") {
      const chartCode = extractTextContent(children).replace(/\n$/, "");
      return <MermaidRenderer chart={chartCode} />;
    }
    return (
      <code className={className} {...rest}>
        {children}
      </code>
    );
  },
};
