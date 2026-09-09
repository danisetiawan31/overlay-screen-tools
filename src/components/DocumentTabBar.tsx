import React from "react";
import { FileText, Plus, X } from "lucide-react";
import type { NoteDocument } from "../bindings";

export interface DocumentTabBarProps {
  documents: NoteDocument[];
  activePath: string | null;
  onSelectTab: (path: string) => void;
  onCloseTab: (path: string) => void;
  onNewTab: () => void;
  isPicking?: boolean;
}

export const DocumentTabBar: React.FC<DocumentTabBarProps> = ({
  documents,
  activePath,
  onSelectTab,
  onCloseTab,
  onNewTab,
  isPicking = false,
}) => {
  return (
    <div
      role="tablist"
      aria-label="Document tabs"
      className="flex items-center gap-1 overflow-x-auto py-1 px-0.5 select-none"
    >
      {documents.map((doc) => {
        const isActive = doc.path === activePath;
        return (
          <div
            key={doc.path}
            role="tab"
            aria-selected={isActive}
            tabIndex={0}
            onClick={() => onSelectTab(doc.path)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onSelectTab(doc.path);
              }
            }}
            title={doc.path}
            className={`group flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs transition-all cursor-pointer border shrink-0 max-w-[180px] ${
              isActive
                ? "bg-zinc-800 text-zinc-100 border-zinc-700 shadow-sm font-medium"
                : "bg-zinc-900/50 text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/60 border-zinc-800/60"
            }`}
          >
            <FileText
              className={`w-3.5 h-3.5 shrink-0 ${
                isActive ? "text-amber-400" : "text-zinc-500 group-hover:text-zinc-400"
              }`}
            />
            <span className="truncate">{doc.title}</span>
            <button
              type="button"
              aria-label={`Tutup tab ${doc.title}`}
              onClick={(e) => {
                e.stopPropagation();
                onCloseTab(doc.path);
              }}
              className="p-0.5 -mr-1 rounded hover:bg-zinc-700 text-zinc-500 hover:text-zinc-200 transition-colors"
            >
              <X className="w-3 h-3" />
            </button>
          </div>
        );
      })}

      <button
        type="button"
        onClick={onNewTab}
        disabled={isPicking}
        aria-label="Buka dokumen baru"
        title="Buka file Markdown baru (+)"
        className="flex items-center justify-center w-6 h-6 rounded-md bg-zinc-900/60 hover:bg-zinc-800 active:bg-zinc-700 text-zinc-400 hover:text-zinc-200 border border-zinc-800/80 transition-colors shrink-0 disabled:opacity-50 disabled:cursor-not-allowed"
      >
        <Plus className="w-3.5 h-3.5" />
      </button>
    </div>
  );
};

export default DocumentTabBar;
