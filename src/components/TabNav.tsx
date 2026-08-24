import React from "react";

export type TabType = "notes" | "qa";

export interface TabNavProps {
  activeTab: TabType;
  onTabChange: (tab: TabType) => void;
  qaAvailable: boolean;
}

export const TabNav: React.FC<TabNavProps> = ({
  activeTab,
  onTabChange,
  qaAvailable,
}) => {
  return (
    <div role="tablist" className="flex items-center space-x-1 border-b border-zinc-800 bg-zinc-900/50 px-3 pt-1">
      <button
        type="button"
        role="tab"
        id="tab-notes"
        aria-selected={activeTab === "notes"}
        aria-controls="panel-notes"
        onClick={() => onTabChange("notes")}
        className={`px-3 py-1.5 text-xs font-medium rounded-t transition-colors border-b-2 ${
          activeTab === "notes"
            ? "border-emerald-500 text-zinc-100 bg-zinc-800/80"
            : "border-transparent text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/40"
        }`}
      >
        Notes
      </button>
      <button
        type="button"
        role="tab"
        id="tab-qa"
        aria-selected={activeTab === "qa"}
        aria-controls="panel-qa"
        disabled={!qaAvailable}
        title={!qaAvailable ? "F8 tidak tersedia (hotkey gagal didaftarkan)" : undefined}
        onClick={() => {
          if (qaAvailable) {
            onTabChange("qa");
          }
        }}
        className={`px-3 py-1.5 text-xs font-medium rounded-t transition-colors border-b-2 ${
          !qaAvailable
            ? "border-transparent text-zinc-600 opacity-40 cursor-not-allowed"
            : activeTab === "qa"
            ? "border-emerald-500 text-zinc-100 bg-zinc-800/80"
            : "border-transparent text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/40"
        }`}
      >
        Live Q&A
        {!qaAvailable && (
          <span className="ml-1.5 text-[10px] px-1 py-0.2 rounded bg-zinc-800 text-zinc-500 border border-zinc-700/50">
            Off
          </span>
        )}
      </button>
    </div>
  );
};

export default TabNav;
