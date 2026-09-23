import { useEffect, useRef } from "react";
import { Search, ChevronUp, ChevronDown, X } from "lucide-react";

export interface FindBarProps {
  query: string;
  onQueryChange: (newQuery: string) => void;
  currentIndex: number;
  totalMatches: number;
  onNext: () => void;
  onPrev: () => void;
  onClose: () => void;
}

export function FindBar({
  query,
  onQueryChange,
  currentIndex,
  totalMatches,
  onNext,
  onPrev,
  onClose,
}: FindBarProps) {
  const inputRef = useRef<HTMLInputElement>(null);

  // Otomatis fokus dan select teks saat FindBar terbuka agar user bisa langsung mengetik
  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      if (e.shiftKey) {
        onPrev();
      } else {
        onNext();
      }
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  };

  const hasQuery = query.trim().length > 0;

  return (
    <div
      role="search"
      aria-label="Cari di dokumen"
      className="absolute top-2 right-2 z-20 flex items-center gap-1.5 bg-[#18181b]/95 backdrop-blur border border-[#3f3f46] p-1.5 rounded-lg shadow-2xl text-xs select-none animate-in fade-in slide-in-from-top-1 duration-150"
    >
      <Search className="w-3.5 h-3.5 text-[#71717a] shrink-0 ml-1" />
      <input
        ref={inputRef}
        type="text"
        value={query}
        onChange={(e) => onQueryChange(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Cari di dokumen..."
        aria-label="Input kata kunci pencarian"
        className="bg-[#27272a] border border-[#3f3f46] rounded px-2 py-1 text-xs text-[#fafafa] placeholder:text-[#71717a] focus:outline-none focus:border-amber-400 w-44 font-sans leading-none"
      />

      {/* Match Counter */}
      {hasQuery && (
        <span
          data-testid="find-counter"
          className={`px-1 text-[11px] font-mono shrink-0 min-w-[3rem] text-center ${
            totalMatches === 0 ? "text-rose-400 font-semibold" : "text-[#a1a1aa]"
          }`}
        >
          {totalMatches === 0 ? "0 / 0" : `${currentIndex + 1} / ${totalMatches}`}
        </span>
      )}

      {/* Navigasi Prev & Next */}
      <button
        type="button"
        onClick={onPrev}
        disabled={totalMatches === 0}
        title="Sebelumnya (Shift+Enter)"
        aria-label="Hasil sebelumnya"
        className="p-1 rounded text-[#a1a1aa] hover:text-[#fafafa] hover:bg-[#27272a] active:bg-[#3f3f46] transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
      >
        <ChevronUp className="w-3.5 h-3.5" />
      </button>
      <button
        type="button"
        onClick={onNext}
        disabled={totalMatches === 0}
        title="Berikutnya (Enter)"
        aria-label="Hasil berikutnya"
        className="p-1 rounded text-[#a1a1aa] hover:text-[#fafafa] hover:bg-[#27272a] active:bg-[#3f3f46] transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
      >
        <ChevronDown className="w-3.5 h-3.5" />
      </button>

      {/* Tombol Tutup */}
      <button
        type="button"
        onClick={onClose}
        title="Tutup (Esc)"
        aria-label="Tutup pencarian"
        className="p-1 rounded text-[#a1a1aa] hover:text-rose-400 hover:bg-[#27272a] active:bg-[#3f3f46] transition-colors ml-0.5"
      >
        <X className="w-3.5 h-3.5" />
      </button>
    </div>
  );
}
