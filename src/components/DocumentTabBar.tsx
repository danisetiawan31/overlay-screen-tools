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
      aria-label="Document tabs (Ctrl+F10 untuk ganti tab)"
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
            title={`${doc.path} (Ctrl+F10 untuk ganti tab)`}
            className={`group flex items-center gap-1.5 px-2.5 py-1 rounded-md text-xs transition-all cursor-pointer border shrink-0 max-w-[180px] ${
              isActive
                ? "bg-[#28282d] text-[#fafafa] border-[#38383e] shadow-xs font-medium"
                : "bg-[#222225]/70 text-[#a1a1aa] hover:text-[#fafafa] hover:bg-[#28282d]/50 border-[#2e2e32]"
            }`}
          >
            <FileText
              className={`w-3.5 h-3.5 shrink-0 ${
                isActive ? "text-amber-400" : "text-[#71717a] group-hover:text-[#a1a1aa]"
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
              className="p-0.5 -mr-1 rounded hover:bg-[#38383e] text-[#71717a] hover:text-[#fafafa] transition-colors"
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
        className="flex items-center justify-center w-6 h-6 rounded-md bg-[#28282d] hover:bg-[#34343a] active:bg-[#3f3f46] text-[#a1a1aa] hover:text-[#fafafa] border border-[#38383e] shadow-2xs transition-colors shrink-0 disabled:opacity-50 disabled:cursor-not-allowed"
      >
        <Plus className="w-3.5 h-3.5" />
      </button>
    </div>
  );
};

export default DocumentTabBar;
