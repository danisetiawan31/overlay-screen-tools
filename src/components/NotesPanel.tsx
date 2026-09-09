import { useEffect, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import { FolderOpen, FileText, Edit3, AlertCircle } from "lucide-react";
import { commands, events, type NoteDocument } from "../bindings";
import { DocumentTabBar } from "./DocumentTabBar";

type NotesSubMode = "file" | "scratchpad";

export function NotesPanel() {
  const [subMode, setSubMode] = useState<NotesSubMode>("file");
  const [documents, setDocuments] = useState<NoteDocument[]>([]);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [unifiedError, setUnifiedError] = useState<string | null>(null);
  const [scratchpadText, setScratchpadText] = useState<string>("");
  const [isPicking, setIsPicking] = useState<boolean>(false);

  // 1. Initial sync saat mount via getNotesState (menutup race condition auto-restore startup)
  useEffect(() => {
    let isMounted = true;

    commands.getNotesState().then((res) => {
      if (!isMounted) return;
      if (res.status === "ok") {
        setDocuments(res.data.documents);
        setActivePath(res.data.activePath);
        if (res.data.error !== null) {
          setUnifiedError(res.data.error);
        } else {
          setUnifiedError(null);
        }
      } else {
        setUnifiedError(res.error);
      }
    });

    return () => {
      isMounted = false;
    };
  }, []);

  // 2. Registrasi event listener notesUpdate dan notesError di File Mode (dengan cleanup ketat)
  useEffect(() => {
    if (subMode !== "file") return;

    let unlistenUpdate: (() => void) | null = null;
    let unlistenError: (() => void) | null = null;
    let isCleanedUp = false;

    events.notesUpdate
      .listen((event) => {
        if (isCleanedUp) return;
        setDocuments((prev) => {
          const idx = prev.findIndex((d) => d.path === event.payload.path);
          if (idx >= 0) {
            const next = [...prev];
            next[idx] = { ...next[idx], content: event.payload.content };
            return next;
          }
          const title = event.payload.path.split(/[/\\]/).pop() || event.payload.path;
          return [...prev, { path: event.payload.path, title, content: event.payload.content }];
        });
        setActivePath((curr) => curr ?? event.payload.path);
        // Event notesUpdate sukses selalu membersihkan banner error (terpadu)
        setUnifiedError(null);
      })
      .then((unsub) => {
        if (isCleanedUp) {
          unsub();
        } else {
          unlistenUpdate = unsub;
        }
      });

    events.notesError
      .listen((event) => {
        if (isCleanedUp) return;
        setUnifiedError(event.payload.message);
      })
      .then((unsub) => {
        if (isCleanedUp) {
          unsub();
        } else {
          unlistenError = unsub;
        }
      });

    return () => {
      isCleanedUp = true;
      if (unlistenUpdate) unlistenUpdate();
      if (unlistenError) unlistenError();
    };
  }, [subMode]);

  // 3. Tab handlers
  const handleSelectTab = (path: string) => {
    setActivePath(path);
    commands.setActiveNotesFile(path).catch((err: unknown) => {
      setUnifiedError(err instanceof Error ? err.message : String(err));
    });
  };

  const handleCloseTab = async (path: string) => {
    try {
      const res = await commands.closeNotesFile(path);
      if (res.status === "error") {
        setUnifiedError(res.error);
      }
    } catch (err: unknown) {
      setUnifiedError(err instanceof Error ? err.message : String(err));
    }
    setDocuments((prev) => {
      const next = prev.filter((d) => d.path !== path);
      if (activePath === path) {
        const closedIdx = prev.findIndex((d) => d.path === path);
        let newActive: string | null = null;
        if (next.length > 0) {
          const candidateIdx = Math.min(closedIdx, next.length - 1);
          newActive = next[candidateIdx].path;
        }
        setActivePath(newActive);
      }
      return next;
    });
  };

  // 4. Handler tombol "Pilih File" / New Tab dengan penanganan 3 cabang
  const handlePickFile = async () => {
    setIsPicking(true);
    try {
      const res = await commands.pickNotesFile();
      if (res.status === "ok") {
        if (res.data !== null) {
          const picked = res.data;
          setDocuments((prev) => {
            const idx = prev.findIndex((d) => d.path === picked.path);
            if (idx >= 0) {
              const next = [...prev];
              next[idx] = { ...next[idx], title: picked.title, content: picked.content };
              return next;
            }
            return [...prev, { path: picked.path, title: picked.title, content: picked.content }];
          });
          setActivePath(picked.path);
          setUnifiedError(null);
        }
        // res.data === null (User klik cancel) -> no-op bersih
      } else {
        // Command-level error (misal invoke gagal)
        setUnifiedError(res.error);
      }
    } catch (err: unknown) {
      setUnifiedError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsPicking(false);
    }
  };

  const activeDocument =
    documents.find((d) => d.path === activePath) ??
    (documents.length > 0 ? documents[0] : null);

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Sub-mode switcher header */}
      <div className="flex items-center justify-between pb-3 border-b border-zinc-800/80 mb-2 select-none">
        <div className="flex items-center gap-1 bg-zinc-900/90 p-1 rounded-lg border border-zinc-800">
          <button
            type="button"
            onClick={() => setSubMode("file")}
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium transition-colors ${
              subMode === "file"
                ? "bg-zinc-800 text-zinc-100 shadow-sm"
                : "text-zinc-400 hover:text-zinc-200"
            }`}
          >
            <FileText className="w-3.5 h-3.5" />
            File Mode
          </button>
          <button
            type="button"
            onClick={() => setSubMode("scratchpad")}
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium transition-colors ${
              subMode === "scratchpad"
                ? "bg-zinc-800 text-zinc-100 shadow-sm"
                : "text-zinc-400 hover:text-zinc-200"
            }`}
          >
            <Edit3 className="w-3.5 h-3.5" />
            Scratchpad
          </button>
        </div>

        {subMode === "file" && (
          <button
            type="button"
            onClick={handlePickFile}
            disabled={isPicking}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium bg-zinc-800 hover:bg-zinc-700 active:bg-zinc-600 text-zinc-200 border border-zinc-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <FolderOpen className="w-3.5 h-3.5 text-zinc-400" />
            {isPicking ? "Memilih..." : "Pilih File"}
          </button>
        )}
      </div>

      {/* Document Tab Bar */}
      {subMode === "file" && documents.length > 0 && (
        <div className="pb-2 border-b border-zinc-800/60 mb-2">
          <DocumentTabBar
            documents={documents}
            activePath={activePath}
            onSelectTab={handleSelectTab}
            onCloseTab={handleCloseTab}
            onNewTab={handlePickFile}
            isPicking={isPicking}
          />
        </div>
      )}

      {/* Unified Persistent Error Banner */}
      {unifiedError && (
        <div
          role="alert"
          className="flex items-start gap-2 bg-amber-950/70 border border-amber-800/80 text-amber-200 px-3 py-2 rounded-lg text-xs mb-3 select-none"
        >
          <AlertCircle className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
          <div className="flex-1 leading-relaxed">
            <span className="font-semibold text-amber-300">Peringatan: </span>
            {unifiedError}
          </div>
        </div>
      )}

      {/* Main Content Area */}
      <div className="flex-1 overflow-y-auto min-h-0">
        {subMode === "file" ? (
          activeDocument !== null ? (
            <div className="prose prose-invert prose-zinc max-w-none text-zinc-200 leading-relaxed break-words space-y-2 [&_h1]:text-lg [&_h1]:font-bold [&_h1]:text-zinc-100 [&_h2]:text-base [&_h2]:font-semibold [&_h2]:text-zinc-100 [&_h3]:text-sm [&_h3]:font-medium [&_h3]:text-zinc-200 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:list-decimal [&_ol]:pl-5 [&_code]:bg-zinc-900 [&_code]:px-1 [&_code]:py-0.5 [&_code]:rounded [&_code]:text-amber-300 [&_pre]:bg-zinc-900/90 [&_pre]:border [&_pre]:border-zinc-800 [&_pre]:p-3 [&_pre]:rounded-lg [&_table]:border-collapse [&_th]:border [&_th]:border-zinc-800 [&_th]:p-1.5 [&_td]:border [&_td]:border-zinc-800 [&_td]:p-1.5">
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                rehypePlugins={[rehypeHighlight]}
              >
                {activeDocument.content}
              </ReactMarkdown>
            </div>
          ) : (
            <div className="flex flex-col items-center justify-center h-full text-center p-6 border border-dashed border-zinc-800 rounded-lg">
              <FileText className="w-8 h-8 text-zinc-600 mb-2" />
              <p className="text-sm font-medium text-zinc-400">Belum ada file notes yang dipilih</p>
              <p className="text-xs text-zinc-500 mt-1 max-w-xs">
                Klik tombol &quot;Pilih File&quot; di atas untuk membuka file Markdown (.md). File akan otomatis ter-update saat di-edit.
              </p>
            </div>
          )
        ) : (
          <div className="h-full flex flex-col">
            <textarea
              value={scratchpadText}
              onChange={(e) => setScratchpadText(e.target.value)}
              placeholder="Tulis catatan ad-hoc di sini (catatan ini bersifat sementara dan tidak disimpan ke disk)..."
              className="w-full h-full bg-zinc-900/60 border border-zinc-800 rounded-lg p-3 text-zinc-100 font-mono resize-none focus:outline-none focus:border-zinc-700 leading-relaxed placeholder:text-zinc-600"
            />
          </div>
        )}
      </div>
    </div>
  );
}
