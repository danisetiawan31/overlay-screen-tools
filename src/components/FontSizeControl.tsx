import React from "react";

export const MIN_FONT_SIZE = 10;
export const MAX_FONT_SIZE = 32;
export const DEFAULT_FONT_SIZE = 14;

export interface FontSizeControlProps {
  fontSize: number;
  onUpdateFontSize: (newSize: number) => void;
  disabled?: boolean;
}

export const FontSizeControl: React.FC<FontSizeControlProps> = ({
  fontSize,
  onUpdateFontSize,
  disabled = false,
}) => {
  const isAtMin = fontSize <= MIN_FONT_SIZE;
  const isAtMax = fontSize >= MAX_FONT_SIZE;

  return (
    <div className="flex items-center space-x-1.5 bg-[#28282d] px-2 py-0.5 rounded border border-[#38383e] text-xs select-none">
      <span className="text-[#a1a1aa] text-[10px] uppercase font-semibold mr-0.5">Font</span>
      <button
        type="button"
        aria-label="Decrease font size"
        disabled={disabled || isAtMin}
        onClick={() => onUpdateFontSize(fontSize - 1)}
        className="w-5 h-5 flex items-center justify-center rounded bg-[#34343a] hover:bg-[#404048] active:bg-[#4a4a52] disabled:opacity-30 disabled:cursor-not-allowed text-[#fafafa] transition-colors font-bold text-xs border border-[#3f3f46]"
      >
        −
      </button>
      <span className="text-xs font-mono text-[#d4d4d8] min-w-[28px] text-center font-medium">
        {fontSize}px
      </span>
      <button
        type="button"
        aria-label="Increase font size"
        disabled={disabled || isAtMax}
        onClick={() => onUpdateFontSize(fontSize + 1)}
        className="w-5 h-5 flex items-center justify-center rounded bg-[#34343a] hover:bg-[#404048] active:bg-[#4a4a52] disabled:opacity-30 disabled:cursor-not-allowed text-[#fafafa] transition-colors font-bold text-xs border border-[#3f3f46]"
      >
        +
      </button>
    </div>
  );
};

export default FontSizeControl;
