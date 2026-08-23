import React from "react";

export const App: React.FC = () => {
  return (
    <div className="flex flex-col h-screen w-screen bg-zinc-950 text-zinc-100 font-sans">
      <header data-tauri-drag-region className="flex items-center justify-between px-3 py-2 bg-zinc-900 border-b border-zinc-800 cursor-move">
        <h1 className="text-xs font-semibold text-zinc-400 select-none">Screen Overlay Tool</h1>
        <div className="flex items-center space-x-1 text-xs text-zinc-500">
          <span className="px-1.5 py-0.5 rounded bg-zinc-800 text-zinc-300">F9</span>
          <span>Toggle</span>
        </div>
      </header>
      <main className="flex-1 p-4 overflow-auto">
        <div className="rounded-lg border border-emerald-500/30 bg-emerald-950/20 p-4 text-center">
          <h2 className="text-emerald-400 font-semibold mb-1">Stealth Overlay Active</h2>
          <p className="text-zinc-400 text-xs">
            Window ini terlindungi oleh <code className="text-emerald-300">SetWindowDisplayAffinity</code>.
          </p>
        </div>
      </main>
    </div>
  );
};

export default App;
