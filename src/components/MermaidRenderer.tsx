import React, { useEffect, useState } from "react";
import mermaid from "mermaid";
import { Copy, Check } from "lucide-react";

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

export const CodeBlockWrapper: React.FC<React.ComponentPropsWithoutRef<"pre">> = ({
  children,
  className,
  ...rest
}) => {
  const [copied, setCopied] = useState(false);

  let language = "";
  let rawCode = "";

  if (React.isValidElement(children)) {
    const childProps = children.props as { className?: string; children?: React.ReactNode };
    const match = /language-(\w+)/.exec(childProps.className || "");
    if (match) {
      language = match[1];
    }
    rawCode = extractTextContent(childProps.children).replace(/\n$/, "");
  } else {
    rawCode = extractTextContent(children).replace(/\n$/, "");
  }

  const handleCopy = async () => {
    if (!rawCode) return;
    try {
      await navigator.clipboard.writeText(rawCode);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Fallback
    }
  };

  return (
    <div className="my-3 rounded-lg border border-zinc-800/90 bg-zinc-950/95 overflow-hidden shadow-sm">
      {/* Code Header with language and copy button */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-zinc-900/90 border-b border-zinc-800/80 text-[11px] select-none">
        <span className="font-mono font-medium text-zinc-400 uppercase tracking-wider text-[10px]">
          {language || "code"}
        </span>
        <button
          type="button"
          onClick={handleCopy}
          className="flex items-center gap-1 px-1.5 py-0.5 rounded text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800 transition text-[11px]"
          aria-label="Salin kode"
          title="Salin kode"
        >
          {copied ? (
            <>
              <Check className="w-3 h-3 text-emerald-400" />
              <span className="text-emerald-400">Tersalin</span>
            </>
          ) : (
            <>
              <Copy className="w-3 h-3" />
              <span>Salin</span>
            </>
          )}
        </button>
      </div>

      {/* Code Body */}
      <pre
        className={`p-3 overflow-x-auto font-mono text-xs leading-relaxed text-zinc-100 ${className || ""}`}
        {...rest}
      >
        {children}
      </pre>
    </div>
  );
};

export const markdownComponents = {
  pre(props: React.ComponentPropsWithoutRef<"pre">) {
    const { children, ...rest } = props;
    if (
      React.isValidElement(children) &&
      typeof (children.props as { className?: string })?.className === "string" &&
      (children.props as { className?: string }).className?.includes("language-mermaid")
    ) {
      return <>{children}</>;
    }
    return <CodeBlockWrapper {...rest}>{children}</CodeBlockWrapper>;
  },
  code(props: React.ComponentPropsWithoutRef<"code">) {
    const { children, className, ...rest } = props;
    const match = /language-(\w+)/.exec(className || "");
    if (match && match[1] === "mermaid") {
      const chartCode = extractTextContent(children).replace(/\n$/, "");
      return <MermaidRenderer chart={chartCode} />;
    }

    const isInline = !className || !className.includes("language-");
    if (isInline) {
      return (
        <code
          className={`rounded bg-zinc-800/80 px-1.5 py-0.5 font-mono text-[12px] text-amber-300 border border-zinc-700/50 ${className || ""}`}
          {...rest}
        >
          {children}
        </code>
      );
    }

    return (
      <code className={className} {...rest}>
        {children}
      </code>
    );
  },
};

