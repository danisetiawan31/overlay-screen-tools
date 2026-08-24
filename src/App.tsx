import React, { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { commands } from "./bindings";
import { FontSizeControl } from "./components/FontSizeControl";
import { TabNav, TabType } from "./components/TabNav";
import { NotesPanel } from "./components/NotesPanel";

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

  const handleUpdateFontSize = async (newSize: number) => {
    try {
      const res = await commands.updateFontSize(newSize);
      if (res.status === "ok") {
        setFontSize(newSize);
        setUpdateError(null);
      } else {
        setUpdateError(res.error);
      }
    } catch (err) {
      setUpdateError(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <div className="flex flex-col h-screen w-screen bg-zinc-950 text-zinc-100 font-sans">
      <header
        data-tauri-drag-region
        className="flex items-center justify-between px-3 py-2 bg-zinc-900 border-b border-zinc-800 cursor-move select-none"
      >
        <h1 className="text-xs font-semibold text-zinc-400 select-none">Screen Overlay Tool</h1>
        <div className="flex items-center space-x-2">
          {fontSize !== null && (
            <FontSizeControl
              fontSize={fontSize}
              onUpdateFontSize={handleUpdateFontSize}
            />
          )}
          <div className="flex items-center space-x-1 text-xs text-zinc-500">
            <span className="px-1.5 py-0.5 rounded bg-zinc-800 text-zinc-300">F9</span>
            <span>Toggle</span>
          </div>
        </div>
      </header>

      {error && (
        <div role="alert" className="bg-red-950/80 border-b border-red-800/80 px-3 py-2 text-xs text-red-300 flex items-center justify-between">
          <span>Gagal memuat status aplikasi: {error}</span>
        </div>
      )}

      {updateError && (
        <div role="alert" className="bg-red-950/80 border-b border-red-800/80 px-3 py-2 text-xs text-red-300 flex items-center justify-between">
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
        <>
          <TabNav
            activeTab={activeTab}
            onTabChange={setActiveTab}
            qaAvailable={qaAvailable === true}
          />
          <main
            className="flex-1 p-4 overflow-hidden min-h-0"
            style={{ fontSize: fontSize ? `${fontSize}px` : undefined }}
          >
            {activeTab === "notes" && (
              <div
                role="tabpanel"
                id="panel-notes"
                aria-labelledby="tab-notes"
                className="h-full"
              >
                <NotesPanel />
              </div>
            )}
            {activeTab === "qa" && (
              <div
                role="tabpanel"
                id="panel-qa"
                aria-labelledby="tab-qa"
                className="h-full flex flex-col items-center justify-center rounded-lg border border-zinc-800/60 bg-zinc-900/30 p-4 text-center"
              >
                <h2 className="text-zinc-300 font-semibold text-sm mb-1">Live Q&A Mode</h2>
                <p className="text-zinc-500 text-xs">Live Q&A mode — segera hadir</p>
              </div>
            )}
          </main>
        </>
      )}
    </div>
  );
};

export default App;
