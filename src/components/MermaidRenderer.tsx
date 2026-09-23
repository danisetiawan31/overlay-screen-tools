import React, { useEffect, useState } from "react";
import mermaid from "mermaid";
import {
  Copy,
  Check,
  Info,
  Lightbulb,
  AlertCircle,
  AlertTriangle,
  OctagonAlert,
} from "lucide-react";
import { commands } from "../bindings";

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
        <pre className="overflow-x-auto text-[#d4d4d8] font-mono bg-[#18181a] p-2 rounded border border-[#38383e]">
          <code>{chart}</code>
        </pre>
      </div>
    );
  }

  if (!svgContent) {
    return (
      <div
        data-testid="mermaid-loading"
        className="my-3 flex items-center justify-center p-3 text-xs text-[#71717a] bg-[#1a1a1d]/50 rounded-lg border border-[#38383e] animate-pulse"
      >
        Memuat diagram Mermaid...
      </div>
    );
  }

  return (
    <div
      data-testid="mermaid-diagram"
      className="my-3 flex justify-center overflow-x-auto rounded-lg border border-[#38383e] bg-[#242428] p-4 select-none [&_svg]:max-w-full [&_svg]:h-auto"
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

/**
 * Menyalin teks ke clipboard sistem operasi.
 * Mendahulukan Tauri native command agar dapat menyalin meskipun window berstatus WS_EX_NOACTIVATE
 * (tanpa focus dokumen Chromium), lalu fallback ke Web API dan execCommand.
 */
export async function copyTextToClipboard(text: string): Promise<boolean> {
  if (!text) return false;

  // 1. Prioritaskan Win32 native clipboard via Tauri IPC commands.
  // Ini bekerja 100% tanpa memerlukan keyboard/window focus (WS_EX_NOACTIVATE aman).
  try {
    const res = await commands.copyToClipboard({ text });
    if (res.status === "ok") {
      return true;
    }
  } catch {
    // Fallback jika bukan environment Tauri (misal browser biasa / vitest)
  }

  // 2. Fallback Web Clipboard API
  try {
    if (typeof navigator !== "undefined" && navigator.clipboard && navigator.clipboard.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // Fallback jika document tidak memiliki fokus
  }

  // 3. Fallback textarea execCommand copy
  try {
    if (typeof document !== "undefined") {
      const textarea = document.createElement("textarea");
      textarea.value = text;
      textarea.style.position = "fixed";
      textarea.style.left = "-9999px";
      textarea.style.top = "-9999px";
      textarea.style.opacity = "0";
      document.body.appendChild(textarea);
      textarea.focus();
      textarea.select();
      const success = document.execCommand("copy");
      document.body.removeChild(textarea);
      if (success) return true;
    }
  } catch {
    // Abaikan
  }

  return false;
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
    const ok = await copyTextToClipboard(rawCode);
    if (ok) {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  return (
    <div className="my-3 rounded-lg border border-[#38383e] bg-[#18181a] overflow-hidden shadow-sm">
      {/* Code Header with language and copy button */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-[#222226] border-b border-[#38383e] text-[11px] select-none">
        <span className="font-mono font-medium text-[#a1a1aa] uppercase tracking-wider text-[10px]">
          {language || "code"}
        </span>
        <button
          type="button"
          onClick={handleCopy}
          className="flex items-center gap-1 px-1.5 py-0.5 rounded text-[#a1a1aa] hover:text-[#fafafa] hover:bg-[#28282d] transition text-[11px]"
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
        className={`p-3 overflow-x-auto font-mono text-xs leading-relaxed text-[#d4d4d8] ${className || ""}`}
        {...rest}
      >
        {children}
      </pre>
    </div>
  );
};

type AlertType = "NOTE" | "TIP" | "IMPORTANT" | "WARNING" | "CAUTION";

const ALERT_CONFIG: Record<
  AlertType,
  {
    title: string;
    icon: React.ComponentType<{ className?: string }>;
    borderColor: string;
    bgColor: string;
    textColor: string;
  }
> = {
  NOTE: {
    title: "Note",
    icon: Info,
    borderColor: "border-sky-500/40 border-l-sky-400",
    bgColor: "bg-sky-950/25",
    textColor: "text-sky-400",
  },
  TIP: {
    title: "Tip",
    icon: Lightbulb,
    borderColor: "border-emerald-500/40 border-l-emerald-400",
    bgColor: "bg-emerald-950/25",
    textColor: "text-emerald-400",
  },
  IMPORTANT: {
    title: "Important",
    icon: AlertCircle,
    borderColor: "border-purple-500/40 border-l-purple-400",
    bgColor: "bg-purple-950/25",
    textColor: "text-purple-400",
  },
  WARNING: {
    title: "Warning",
    icon: AlertTriangle,
    borderColor: "border-amber-500/40 border-l-amber-400",
    bgColor: "bg-amber-950/25",
    textColor: "text-amber-400",
  },
  CAUTION: {
    title: "Caution",
    icon: OctagonAlert,
    borderColor: "border-rose-500/40 border-l-rose-400",
    bgColor: "bg-rose-950/25",
    textColor: "text-rose-400",
  },
};

const ALERT_REGEX = /^\s*\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]/i;

function parseAlertFromChildren(children: React.ReactNode): {
  alertType: AlertType | null;
  content: React.ReactNode;
} {
  const childArray = React.Children.toArray(children);
  if (childArray.length === 0) {
    return { alertType: null, content: children };
  }

  // Lewati string yang hanya berisi newline/whitespace dari parser AST
  const firstMeaningfulIndex = childArray.findIndex(
    (c) => !(typeof c === "string" && c.trim() === "")
  );

  if (firstMeaningfulIndex === -1) {
    return { alertType: null, content: children };
  }

  const firstNode = childArray[firstMeaningfulIndex];

  // Skenario 1: firstNode adalah string langsung
  if (typeof firstNode === "string") {
    const match = firstNode.match(ALERT_REGEX);
    if (match) {
      const alertType = match[1].toUpperCase() as AlertType;
      const remainder = firstNode.slice(match[0].length).replace(/^\s*\n?/, "");
      const newChildren = [
        ...childArray.slice(0, firstMeaningfulIndex),
        ...(remainder ? [remainder] : []),
        ...childArray.slice(firstMeaningfulIndex + 1),
      ];
      return { alertType, content: newChildren };
    }
  }

  // Skenario 2: firstNode adalah ReactElement (umumnya <p>)
  if (React.isValidElement(firstNode)) {
    const elementChildren = React.Children.toArray(
      (firstNode.props as { children?: React.ReactNode }).children
    );

    const firstTextIndex = elementChildren.findIndex(
      (c) => !(typeof c === "string" && c.trim() === "")
    );

    if (
      firstTextIndex !== -1 &&
      typeof elementChildren[firstTextIndex] === "string"
    ) {
      const text = elementChildren[firstTextIndex];
      const match = text.match(ALERT_REGEX);
      if (match) {
        const alertType = match[1].toUpperCase() as AlertType;
        const remainder = text
          .slice(match[0].length)
          .replace(/^\s*\n?/, "");

        let newFirstNode: React.ReactNode = null;
        const newElementChildren = [
          ...elementChildren.slice(0, firstTextIndex),
          ...(remainder ? [remainder] : []),
          ...elementChildren.slice(firstTextIndex + 1),
        ].filter((c) => !(typeof c === "string" && c.trim() === ""));

        if (newElementChildren.length > 0) {
          newFirstNode = React.cloneElement(
            firstNode as React.ReactElement<{ children?: React.ReactNode }>,
            {},
            ...newElementChildren
          );
        }

        const newContent = [
          ...childArray.slice(0, firstMeaningfulIndex),
          ...(newFirstNode ? [newFirstNode] : []),
          ...childArray.slice(firstMeaningfulIndex + 1),
        ];

        return { alertType, content: newContent };
      }
    }
  }

  return { alertType: null, content: children };
}

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
          className={`rounded bg-[#2e2e34] px-1.5 py-0.5 font-mono text-[12px] text-amber-300 border border-[#45454e] ${className || ""}`}
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
  blockquote(props: React.ComponentPropsWithoutRef<"blockquote">) {
    const { children, className, ...rest } = props;
    const { alertType, content } = parseAlertFromChildren(children);

    if (alertType) {
      const config = ALERT_CONFIG[alertType];
      const Icon = config.icon;

      return (
        <div
          role="region"
          aria-label={`${config.title} alert`}
          className={`my-3 rounded-r-lg border border-l-4 ${config.borderColor} ${config.bgColor} p-3.5 transition-colors select-text not-italic`}
        >
          <div
            className={`flex items-center gap-1.5 font-bold uppercase tracking-wider text-[11px] mb-2 ${config.textColor} select-none`}
          >
            <Icon className="w-4 h-4 shrink-0" />
            <span>{config.title}</span>
          </div>
          <div className="text-[#d4d4d8] leading-relaxed space-y-2.5 [&_strong]:text-white [&_strong]:font-bold [&_h1]:text-base [&_h1]:font-bold [&_h1]:text-[#fafafa] [&_h2]:text-sm [&_h2]:font-semibold [&_h2]:text-[#fafafa] [&_h3]:text-xs [&_h3]:font-medium [&_h3]:text-[#f4f4f5] [&_p]:mb-1.5 [&>*:last-child]:mb-0">
            {content}
          </div>
        </div>
      );
    }

    return (
      <blockquote className={className} {...rest}>
        {children}
      </blockquote>
    );
  },
};

