import React, { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { commands } from "./bindings";
import {
  FontSizeControl,
  MIN_FONT_SIZE,
  MAX_FONT_SIZE,
  DEFAULT_FONT_SIZE,
} from "./components/FontSizeControl";
import { TabNav, TabType } from "./components/TabNav";
import { NotesPanel } from "./components/NotesPanel";
import { QaPanel } from "./components/QaPanel";
import appLogo from "./assets/icon-dani.svg";

export const App: React.FC = () => {
  const [fontSize, setFontSize] = useState<number | null>(null);
  const [qaAvailable, setQaAvailable] = useState<boolean | null>(null);
  const [activeTab, setActiveTab] = useState<TabType>("notes");
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);

  // Ref untuk menjaga nilai qaAvailable terkini agar handler listener event
  // tidak mengalami stale closure tanpa harus re-subscribe listener berulang kali.
  const qaAvailableRef = useRef<boolean | null>(qaAvailable);
  useEffect(() => {
    qaAvailableRef.current = qaAvailable;
  }, [qaAvailable]);

  useEffect(() => {
    let isMounted = true;

    async function initAppState() {
      try {
        const res = await commands.getAppState();
        if (!isMounted) return;

        if (res.status === "ok") {
          setFontSize(res.data.fontSize);
          setQaAvailable(res.data.qaAvailable);
          setIsLoading(false);
        } else {
          setError(res.error);
          setIsLoading(false);
        }
      } catch (err) {
        if (!isMounted) return;
        setError(err instanceof Error ? err.message : String(err));
        setIsLoading(false);
      }
    }

    initAppState();

    return () => {
      isMounted = false;
    };
  }, []);

  // Listen event qa:recording-started untuk auto-switch ke tab Q&A
  useEffect(() => {
    let isMounted = true;
    let unlistenFn: (() => void) | undefined;

    // Catatan Langkah 0: Event qa:recording-started tidak memiliki payload struct di Rust
    // sehingga tidak lewat collect_events! tauri-specta. Kita menggunakan listen() mentah
    // dari @tauri-apps/api/event sesuai docs/api-contract.md §2.
    listen("qa:recording-started", () => {
      // Baca nilai qaAvailable terkini dari ref (bebas stale closure)
      if (isMounted && qaAvailableRef.current === true) {
        setActiveTab("qa");
      }
    }).then((unlisten) => {
      if (!isMounted) {
        unlisten();
      } else {
        unlistenFn = unlisten;
      }
    });

    return () => {
      isMounted = false;
      if (unlistenFn) {
        unlistenFn();
      }
    };
  }, []);

  // Listen event tab:toggle (F6) untuk bolak-balik antara tab Notes dan Live Q&A
  useEffect(() => {
    let isMounted = true;
    let unlistenFn: (() => void) | undefined;

    listen("tab:toggle", () => {
      if (isMounted) {
        setActiveTab((prev) => {
          if (prev === "notes") {
            return qaAvailableRef.current ? "qa" : "notes";
          } else {
            return "notes";
          }
        });
      }
    }).then((unlisten) => {
      if (!isMounted) {
        unlisten();
      } else {
        unlistenFn = unlisten;
      }
    });

    return () => {
      isMounted = false;
      if (unlistenFn) {
        unlistenFn();
      }
    };
  }, []);

  const handleUpdateFontSize = async (newSize: number) => {
    setFontSize(newSize);
    try {
      const res = await commands.updateFontSize(newSize);
      if (res.status === "ok") {
        setUpdateError(null);
      } else {
        setUpdateError(res.error);
      }
    } catch (err) {
      setUpdateError(err instanceof Error ? err.message : String(err));
    }
  };

  // In-App Keyboard & Wheel Shortcuts for Font Zoom (Ctrl +, Ctrl -, Ctrl 0, Ctrl + Wheel)
  useEffect(() => {
    if (fontSize === null) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      const isCtrlOrCmd = e.ctrlKey || e.metaKey;
      if (!isCtrlOrCmd) return;

      if (e.key === "+" || e.key === "=" || e.key === "Add") {
        e.preventDefault();
        const nextSize = Math.min(MAX_FONT_SIZE, fontSize + 1);
        if (nextSize !== fontSize) {
          handleUpdateFontSize(nextSize);
        }
      } else if (e.key === "-" || e.key === "_" || e.key === "Subtract") {
        e.preventDefault();
        const nextSize = Math.max(MIN_FONT_SIZE, fontSize - 1);
        if (nextSize !== fontSize) {
          handleUpdateFontSize(nextSize);
        }
      } else if (e.key === "0" || e.key === "Numpad0") {
        e.preventDefault();
        if (fontSize !== DEFAULT_FONT_SIZE) {
          handleUpdateFontSize(DEFAULT_FONT_SIZE);
        }
      }
    };

    const handleWheel = (e: WheelEvent) => {
      const isCtrlOrCmd = e.ctrlKey || e.metaKey;
      if (!isCtrlOrCmd) return;

      // Prevent default webview whole-page zoom
      e.preventDefault();

      if (e.deltaY < 0) {
        const nextSize = Math.min(MAX_FONT_SIZE, fontSize + 1);
        if (nextSize !== fontSize) {
          handleUpdateFontSize(nextSize);
        }
      } else if (e.deltaY > 0) {
        const nextSize = Math.max(MIN_FONT_SIZE, fontSize - 1);
        if (nextSize !== fontSize) {
          handleUpdateFontSize(nextSize);
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("wheel", handleWheel, { passive: false });

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("wheel", handleWheel);
    };
  }, [fontSize]);

  return (
    <div className="flex flex-col h-screen w-screen bg-zinc-950 text-zinc-100 font-sans">
      {/* Unified Compact Top Bar: Tabs di kiri, Font Control & F9 Toggle di kanan */}
      <header
        data-tauri-drag-region
        className="flex items-center justify-between px-3 pt-1 border-b border-zinc-800 bg-zinc-900/90 cursor-move select-none shrink-0"
      >
        {/* Sisi Kiri: Logo mini + Tab Navigation */}
        <div className="flex items-center space-x-2">
          <img
            src={appLogo}
            alt="App Logo"
            className="w-3.5 h-3.5 rounded-sm object-contain opacity-70 mb-0.5 select-none pointer-events-none"
          />
          <TabNav
            activeTab={activeTab}
            onTabChange={setActiveTab}
            qaAvailable={qaAvailable === true}
          />
        </div>

        {/* Sisi Kanan: Font Size Control & F9 Toggle Badge */}
        <div className="flex items-center space-x-2 pb-1">
          {fontSize !== null && (
            <FontSizeControl
              fontSize={fontSize}
              onUpdateFontSize={handleUpdateFontSize}
            />
          )}
          <div className="flex items-center space-x-1 text-xs text-zinc-500">
            <span className="px-1.5 py-0.5 rounded bg-zinc-800 text-zinc-300 font-mono">F9</span>
            <span>Toggle</span>
          </div>
        </div>
      </header>

      {error && (
        <div role="alert" className="bg-red-950/80 border-b border-red-800/80 px-3 py-2 text-xs text-red-300 flex items-center justify-between shrink-0">
          <span>Gagal memuat status aplikasi: {error}</span>
        </div>
      )}

      {updateError && (
        <div role="alert" className="bg-red-950/80 border-b border-red-800/80 px-3 py-2 text-xs text-red-300 flex items-center justify-between shrink-0">
          <span>Gagal mengubah ukuran font: {updateError}</span>
          <button
            type="button"
            onClick={() => setUpdateError(null)}
            className="text-red-400 hover:text-red-200 ml-2 font-bold"
            aria-label="Tutup error"
          >
            ×
          </button>
        </div>
      )}

      {isLoading ? (
        <div className="flex-1 flex items-center justify-center text-xs text-zinc-500">
          <span>Memuat status aplikasi...</span>
        </div>
      ) : (
        <main
          className="flex-1 p-4 overflow-hidden min-h-0"
          style={{ fontSize: fontSize ? `${fontSize}px` : undefined }}
        >
            <div
              role="tabpanel"
              id="panel-notes"
              aria-labelledby="tab-notes"
              className={`h-full ${activeTab === "notes" ? "" : "hidden"}`}
            >
              <NotesPanel />
            </div>
            <div
              role="tabpanel"
              id="panel-qa"
              aria-labelledby="tab-qa"
              className={`h-full ${activeTab === "qa" ? "" : "hidden"}`}
            >
              <QaPanel qaAvailable={qaAvailable === true} />
            </div>
          </main>
      )}
    </div>
  );
};

export default App;
