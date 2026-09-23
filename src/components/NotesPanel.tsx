import { useEffect, useState, useRef, useLayoutEffect } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import { FolderOpen, FileText, Edit3, AlertCircle, Search } from "lucide-react";
import { commands, events, type NoteDocument } from "../bindings";
import { DocumentTabBar } from "./DocumentTabBar";
import { markdownComponents } from "./MermaidRenderer";
import { FindBar } from "./FindBar";

type NotesSubMode = "file" | "scratchpad";

export function NotesPanel() {
  const [subMode, setSubMode] = useState<NotesSubMode>("file");
  const [documents, setDocuments] = useState<NoteDocument[]>([]);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [unifiedError, setUnifiedError] = useState<string | null>(null);
  const [scratchpadText, setScratchpadText] = useState<string>("");
  const [isPicking, setIsPicking] = useState<boolean>(false);

  // In-document Find states
  const [isFindOpen, setIsFindOpen] = useState<boolean>(false);
  const [findQuery, setFindQuery] = useState<string>("");
  const [findMatchIndex, setFindMatchIndex] = useState<number>(0);
  const [findRanges, setFindRanges] = useState<Range[]>([]);

  // Per-tab scroll retention & container references
  const scrollPositionsRef = useRef<Record<string, number>>({});
  const containerRefs = useRef<Record<string, HTMLDivElement | null>>({});

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
    // Bersihkan cache scroll untuk tab yang ditutup
    delete scrollPositionsRef.current[path];
    delete containerRefs.current[path];

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

  // 4. Global Hotkey listener (Ctrl+F10): cycle document tab tanpa memerlukan window focus
  useEffect(() => {
    if (subMode !== "file" || documents.length <= 1 || !events.notesCycleTab?.listen) return;

    let unlisten: (() => void) | null = null;
    let isCleanedUp = false;

    events.notesCycleTab
      .listen(() => {
        if (isCleanedUp) return;
        const normalize = (p: string | null | undefined) =>
          p ? p.replace(/\\/g, "/").toLowerCase() : "";
        const currentEffectivePath = activePath ?? documents[0]?.path ?? null;

        const currentIndex = documents.findIndex(
          (d) => normalize(d.path) === normalize(currentEffectivePath)
        );

        const nextIndex =
          currentIndex === -1 ? 0 : (currentIndex + 1) % documents.length;
        handleSelectTab(documents[nextIndex].path);
      })
      .then((unsub) => {
        if (isCleanedUp) {
          unsub();
        } else {
          unlisten = unsub;
        }
      });

    return () => {
      isCleanedUp = true;
      if (unlisten) unlisten();
    };
  }, [subMode, documents, activePath]);

  // 5. Global Hotkey listener (Ctrl+F11): toggle Find Bar
  useEffect(() => {
    if (subMode !== "file" || !events.notesToggleFind?.listen) return;

    let unlisten: (() => void) | null = null;
    let isCleanedUp = false;

    events.notesToggleFind
      .listen(() => {
        if (isCleanedUp) return;
        setIsFindOpen((prev) => !prev);
      })
      .then((unsub) => {
        if (isCleanedUp) {
          unsub();
        } else {
          unlisten = unsub;
        }
      });

    return () => {
      isCleanedUp = true;
      if (unlisten) unlisten();
    };
  }, [subMode]);

  // 6. In-App Keyboard navigation fallback: Ctrl (+ Shift) + Arrow / Ctrl + Tab untuk switch tab dokumen saat webview terfokus
  useEffect(() => {
    if (subMode !== "file" || documents.length <= 1) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable)
      ) {
        return;
      }

      const isCtrlOrCmd = e.ctrlKey || e.metaKey;
      if (!isCtrlOrCmd) return;

      const isRightArrow =
        e.key === "ArrowRight" ||
        e.code === "ArrowRight" ||
        e.key === "Right" ||
        e.keyCode === 39;
      const isLeftArrow =
        e.key === "ArrowLeft" ||
        e.code === "ArrowLeft" ||
        e.key === "Left" ||
        e.keyCode === 37;
      const isTab = e.key === "Tab" || e.code === "Tab" || e.keyCode === 9;
      const isPageDown = e.key === "PageDown" || e.code === "PageDown";
      const isPageUp = e.key === "PageUp" || e.code === "PageUp";

      const isNext = isRightArrow || (isTab && !e.shiftKey) || isPageDown;
      const isPrev = isLeftArrow || (isTab && e.shiftKey) || isPageUp;

      if (!isNext && !isPrev) return;

      e.preventDefault();
      e.stopPropagation();

      // Normalisasi perbandingan path (case-insensitive & slash-insensitive untuk Windows)
      const normalize = (p: string | null | undefined) =>
        p ? p.replace(/\\/g, "/").toLowerCase() : "";
      const currentEffectivePath = activePath ?? documents[0]?.path ?? null;

      const currentIndex = documents.findIndex(
        (d) => normalize(d.path) === normalize(currentEffectivePath)
      );

      if (isNext) {
        const nextIndex =
          currentIndex === -1 ? 0 : (currentIndex + 1) % documents.length;
        handleSelectTab(documents[nextIndex].path);
      } else if (isPrev) {
        const prevIndex =
          currentIndex === -1
            ? documents.length - 1
            : (currentIndex - 1 + documents.length) % documents.length;
        handleSelectTab(documents[prevIndex].path);
      }
    };

    window.addEventListener("keydown", handleKeyDown, true);
    return () => {
      window.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [subMode, documents, activePath]);

  // 5. Handler tombol "Pilih File" / New Tab dengan penanganan 3 cabang
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

  // Synchronous scroll restoration saat tab aktif berubah atau mode file kembali aktif
  useLayoutEffect(() => {
    if (subMode !== "file") return;
    const currentPath = activeDocument?.path;
    if (currentPath && containerRefs.current[currentPath]) {
      const savedScroll = scrollPositionsRef.current[currentPath];
      if (typeof savedScroll === "number") {
        containerRefs.current[currentPath]!.scrollTop = savedScroll;
      }
    }
  }, [subMode, activePath, activeDocument]);

  // 7. Find Engine: cari teks di dokumen aktif saat query atau dokumen berubah
  useEffect(() => {
    if (!isFindOpen || subMode !== "file" || !activeDocument || !findQuery.trim()) {
      setFindRanges([]);
      setFindMatchIndex(0);
      if (typeof CSS !== "undefined" && "highlights" in CSS) {
        CSS.highlights.delete("search-results");
        CSS.highlights.delete("search-current");
      }
      return;
    }

    const container = containerRefs.current[activeDocument.path];
    if (!container) return;

    const queryLower = findQuery.toLowerCase();
    const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
    const matchedRanges: Range[] = [];

    let node = walker.nextNode();
    while (node) {
      const text = (node.textContent || "").toLowerCase();
      let pos = 0;
      while ((pos = text.indexOf(queryLower, pos)) !== -1) {
        try {
          const range = document.createRange();
          range.setStart(node, pos);
          range.setEnd(node, pos + queryLower.length);
          matchedRanges.push(range);
        } catch {
          // Abaikan jika node tidak valid
        }
        pos += queryLower.length;
      }
      node = walker.nextNode();
    }

    setFindRanges(matchedRanges);
    setFindMatchIndex((prev) =>
      matchedRanges.length > 0 ? (prev < matchedRanges.length ? prev : 0) : 0
    );
  }, [isFindOpen, subMode, activeDocument, findQuery]);

  // 8. Aplikasikan CSS Highlights dan auto-scroll ke match aktif
  useEffect(() => {
    if (
      typeof CSS === "undefined" ||
      !("highlights" in CSS) ||
      typeof Highlight === "undefined"
    ) {
      return;
    }

    if (!isFindOpen || findRanges.length === 0) {
      CSS.highlights.delete("search-results");
      CSS.highlights.delete("search-current");
      return;
    }

    try {
      const allHighlight = new Highlight(...findRanges);
      CSS.highlights.set("search-results", allHighlight);

      if (findMatchIndex >= 0 && findMatchIndex < findRanges.length) {
        const activeRange = findRanges[findMatchIndex];
        const currentHighlight = new Highlight(activeRange);
        CSS.highlights.set("search-current", currentHighlight);
        activeRange.startContainer.parentElement?.scrollIntoView({
          behavior: "instant",
          block: "center",
        });
      } else {
        CSS.highlights.delete("search-current");
      }
    } catch {
      // Abaikan jika range tidak valid
    }
  }, [isFindOpen, findRanges, findMatchIndex]);

  // 9. Bersihkan highlight saat komponen Notes unmount
  useEffect(() => {
    return () => {
      if (typeof CSS !== "undefined" && "highlights" in CSS) {
        CSS.highlights.delete("search-results");
        CSS.highlights.delete("search-current");
      }
    };
  }, []);

  const handleFindNext = () => {
    if (findRanges.length === 0) return;
    setFindMatchIndex((prev) => (prev + 1) % findRanges.length);
  };

  const handleFindPrev = () => {
    if (findRanges.length === 0) return;
    setFindMatchIndex((prev) => (prev - 1 + findRanges.length) % findRanges.length);
  };

  const handleFindClose = () => {
    setIsFindOpen(false);
    setFindQuery("");
    setFindRanges([]);
    setFindMatchIndex(0);
    if (typeof CSS !== "undefined" && "highlights" in CSS) {
      CSS.highlights.delete("search-results");
      CSS.highlights.delete("search-current");
    }
  };

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Sub-mode switcher header */}
      <div className="flex items-center justify-between pb-3 border-b border-[#2e2e32] mb-2 select-none">
        <div className="flex items-center gap-1 bg-[#18181a] p-1 rounded-lg border border-[#2e2e32]">
          <button
            type="button"
            onClick={() => setSubMode("file")}
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-xs font-medium transition-colors ${
              subMode === "file"
                ? "bg-[#28282d] text-[#fafafa] shadow-xs font-semibold"
                : "text-[#a1a1aa] hover:text-[#fafafa] hover:bg-[#28282d]/50"
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
                ? "bg-[#28282d] text-[#fafafa] shadow-xs font-semibold"
                : "text-[#a1a1aa] hover:text-[#fafafa] hover:bg-[#28282d]/50"
            }`}
          >
            <Edit3 className="w-3.5 h-3.5" />
            Scratchpad
          </button>
        </div>

        {subMode === "file" && (
          <div className="flex items-center gap-1.5">
            {documents.length > 0 && (
              <button
                type="button"
                onClick={() => setIsFindOpen((prev) => !prev)}
                title="Cari di dokumen (Ctrl+F11)"
                aria-label="Cari di dokumen"
                className={`flex items-center justify-center px-2 py-1.5 rounded-md text-xs font-medium border shadow-xs transition-colors ${
                  isFindOpen
                    ? "bg-amber-500/20 text-amber-300 border-amber-500/40"
                    : "bg-[#28282d] hover:bg-[#34343a] text-[#a1a1aa] hover:text-[#fafafa] border-[#38383e]"
                }`}
              >
                <Search className="w-3.5 h-3.5" />
              </button>
            )}
            <button
              type="button"
              onClick={handlePickFile}
              disabled={isPicking}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium bg-[#28282d] hover:bg-[#34343a] active:bg-[#3f3f46] text-[#d4d4d8] border border-[#38383e] shadow-xs transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <FolderOpen className="w-3.5 h-3.5 text-[#a1a1aa]" />
              {isPicking ? "Memilih..." : "Pilih File"}
            </button>
          </div>
        )}
      </div>

      {/* Document Tab Bar */}
      {subMode === "file" && documents.length > 0 && (
        <div className="pb-2 border-b border-[#2e2e32] mb-2">
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
      <div className="relative flex-1 min-h-0 flex flex-col overflow-hidden">
        {isFindOpen && subMode === "file" && documents.length > 0 && (
          <FindBar
            query={findQuery}
            onQueryChange={setFindQuery}
            currentIndex={findMatchIndex}
            totalMatches={findRanges.length}
            onNext={handleFindNext}
            onPrev={handleFindPrev}
            onClose={handleFindClose}
          />
        )}

        {subMode === "scratchpad" ? (
          <div className="flex-1 overflow-y-auto min-h-0 h-full flex flex-col">
          <textarea
            value={scratchpadText}
            onChange={(e) => setScratchpadText(e.target.value)}
            placeholder="Tulis catatan ad-hoc di sini (catatan ini bersifat sementara dan tidak disimpan ke disk)..."
            className="w-full h-full bg-[#28282d] border border-[#38383e] rounded-lg p-3 text-[#fafafa] font-mono resize-none focus:outline-none focus:border-emerald-500 leading-relaxed placeholder:text-[#71717a]"
          />
        </div>
      ) : documents.length === 0 ? (
        <div className="flex-1 overflow-y-auto min-h-0">
          <div className="flex flex-col items-center justify-center h-full text-center p-6 border border-dashed border-[#38383e] rounded-lg bg-[#1a1a1d]/50">
            <FileText className="w-8 h-8 text-[#71717a] mb-2" />
            <p className="text-sm font-medium text-[#a1a1aa]">Belum ada file notes yang dipilih</p>
            <p className="text-xs text-[#71717a] mt-1 max-w-xs">
              Klik tombol &quot;Pilih File&quot; di atas untuk membuka file Markdown (.md). File akan otomatis ter-update saat di-edit.
            </p>
          </div>
        </div>
      ) : (
        documents.map((doc) => {
          const isActive = doc.path === (activeDocument?.path ?? documents[0]?.path);
          return (
            <div
              key={doc.path}
              ref={(el) => {
                containerRefs.current[doc.path] = el;
              }}
              onScroll={(e) => {
                scrollPositionsRef.current[doc.path] = e.currentTarget.scrollTop;
              }}
              hidden={!isActive}
              style={!isActive ? { display: "none" } : undefined}
              data-testid={`doc-container-${doc.path}`}
              className={`flex-1 overflow-y-auto min-h-0 ${isActive ? "block" : "hidden"}`}
            >
              <div className="prose prose-invert max-w-none text-[#d4d4d8] leading-relaxed break-words space-y-2.5 [&_strong]:text-white [&_strong]:font-bold [&_h1]:text-base [&_h1]:font-bold [&_h1]:text-[#fafafa] [&_h2]:text-sm [&_h2]:font-semibold [&_h2]:text-[#fafafa] [&_h3]:text-xs [&_h3]:font-medium [&_h3]:text-[#f4f4f5] [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:list-decimal [&_ol]:pl-5 [&_code]:bg-[#2e2e34] [&_code]:px-1.5 [&_code]:py-0.5 [&_code]:rounded [&_code]:text-amber-300 [&_code]:border [&_code]:border-[#45454e] [&_code]:font-mono [&_code]:text-xs [&_pre]:bg-[#18181a] [&_pre]:border [&_pre]:border-[#38383e] [&_pre]:p-3 [&_pre]:rounded-lg [&_table]:border-collapse [&_th]:border [&_th]:border-[#38383e] [&_th]:p-1.5 [&_th]:bg-[#242428] [&_th]:text-[#fafafa] [&_td]:border [&_td]:border-[#38383e] [&_td]:p-1.5 [&_td]:text-[#d4d4d8] [&_blockquote]:border-l-[3px] [&_blockquote]:border-l-[#585862] [&_blockquote]:border-t-0 [&_blockquote]:border-r-0 [&_blockquote]:border-b-0 [&_blockquote]:bg-[#28282d] [&_blockquote]:px-4 [&_blockquote]:py-2.5 [&_blockquote]:rounded-r-md [&_blockquote]:my-2.5 [&_blockquote]:text-[#d4d4d8] [&_blockquote]:italic">
                <ReactMarkdown
                  remarkPlugins={[remarkGfm]}
                  rehypePlugins={[rehypeHighlight]}
                  components={markdownComponents}
                >
                  {doc.content}
                </ReactMarkdown>
              </div>
            </div>
          );
        })
      )}
      </div>
    </div>
  );
}
