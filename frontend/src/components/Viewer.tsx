import { useEffect, useMemo, useRef, useState } from 'react';

import { clamp, formatOffset } from '../lib/format';
import { useViewportData } from '../hooks/useViewportData';
import { getRowIndexForBitOffset, getRowSourceBitOffset, getTotalRows } from '../lib/viewport';
import type { JumpRequest, ViewerResource, ViewportRow } from '../types';
import { BitCanvas } from './BitCanvas';

interface ViewerProps {
  resource: ViewerResource;
  rowWidthBits: number;
  bitSize: number;
  jumpRequest: JumpRequest | null;
  onVisibleOffsetsChange: (bitOffset: number, byteOffset: number, rows: number) => void;
}

const VIEWER_HEIGHT = 560;
const OVERSCAN_ROWS = 12;
const MIN_TEXT_ROW_HEIGHT = 18;

type PaneKind = 'bit' | 'hex' | 'ascii';

function renderOffset(row: ViewportRow): string {
  return `${formatOffset(row.byteOffsetStart)}  ${row.hex}`;
}

export function Viewer({ resource, rowWidthBits, bitSize, jumpRequest, onVisibleOffsetsChange }: ViewerProps) {
  const bitRef = useRef<HTMLDivElement | null>(null);
  const hexRef = useRef<HTMLDivElement | null>(null);
  const asciiRef = useRef<HTMLDivElement | null>(null);
  const syncSource = useRef<PaneKind | null>(null);
  const textRowHeight = Math.max(bitSize, MIN_TEXT_ROW_HEIGHT);

  const totalRows = Math.max(
    1,
    getTotalRows({
      logicalBitLength: resource.logicalBitLength,
      rowWidthBits,
      groupBitLengths: resource.groupBitLengths ?? undefined,
    }),
  );
  const bitTotalHeight = totalRows * bitSize;
  const textTotalHeight = totalRows * textRowHeight;
  const visibleRowCount = Math.min(totalRows, Math.ceil(VIEWER_HEIGHT / bitSize) + OVERSCAN_ROWS * 2);
  const [scrollRowPosition, setScrollRowPosition] = useState(0);

  function getRowHeight(kind: PaneKind): number {
    return kind === 'bit' ? bitSize : textRowHeight;
  }

  function getMaxScrollTop(kind: PaneKind): number {
    const totalHeight = kind === 'bit' ? bitTotalHeight : textTotalHeight;
    return Math.max(0, totalHeight - VIEWER_HEIGHT);
  }

  const startRow = useMemo(() => {
    const raw = Math.floor(scrollRowPosition) - OVERSCAN_ROWS;
    return clamp(raw, 0, Math.max(0, totalRows - visibleRowCount));
  }, [scrollRowPosition, totalRows, visibleRowCount]);

  const { data, loading, error } = useViewportData({
    resourceId: resource.resourceId,
    resourceKind: resource.resourceKind,
    fileSizeBytes: resource.sizeBytes,
    logicalBitLength: resource.logicalBitLength,
    startRow,
    visibleRows: visibleRowCount,
    rowWidthBits,
    groupBitLengths: resource.groupBitLengths ?? undefined,
    useLogicalRowBytes: resource.isFiltered,
  });
  const bitTopOffset = startRow * bitSize;
  const textTopOffset = startRow * textRowHeight;

  useEffect(() => {
    const visibleBitOffset = getRowSourceBitOffset({
      logicalBitLength: resource.logicalBitLength,
      rowWidthBits,
      rowIndex: Math.floor(scrollRowPosition),
      groupBitLengths: resource.groupBitLengths ?? undefined,
    });
    onVisibleOffsetsChange(visibleBitOffset, Math.floor(visibleBitOffset / 8), data?.actualRows ?? 0);
  }, [data?.actualRows, onVisibleOffsetsChange, resource.groupBitLengths, resource.logicalBitLength, rowWidthBits, scrollRowPosition]);

  useEffect(() => {
    if (!jumpRequest) {
      return;
    }

    const targetBitOffset = jumpRequest.mode === 'byte' ? jumpRequest.value * 8 : jumpRequest.value;
    const targetRow = getRowIndexForBitOffset({
      bitOffset: targetBitOffset,
      rowWidthBits,
      groupBitLengths: resource.groupBitLengths ?? undefined,
    });
    syncScroll('bit', targetRow);
  }, [jumpRequest, resource.groupBitLengths, rowWidthBits]);

  function syncScroll(source: PaneKind, nextScrollRow: number) {
    if (syncSource.current && syncSource.current !== source) {
      return;
    }

    syncSource.current = source;
    const clampedRow = clamp(nextScrollRow, 0, Math.max(0, totalRows - 1));
    setScrollRowPosition(clampedRow);

    const targets = [
      { kind: 'bit', element: bitRef.current },
      { kind: 'hex', element: hexRef.current },
      { kind: 'ascii', element: asciiRef.current },
    ] as const;

    targets.forEach(({ kind, element }) => {
      if (!element) {
        return;
      }
      const nextScrollTop = clamp(clampedRow * getRowHeight(kind), 0, getMaxScrollTop(kind));
      if (Math.abs(element.scrollTop - nextScrollTop) > 1) {
        element.scrollTop = nextScrollTop;
      }
    });

    requestAnimationFrame(() => {
      syncSource.current = null;
    });
  }

  function handleScroll(source: PaneKind, event: React.UIEvent<HTMLDivElement>) {
    const rowHeight = getRowHeight(source);
    const nextScrollRow = event.currentTarget.scrollTop / rowHeight;
    if (Math.abs(nextScrollRow - scrollRowPosition) <= 0.01 && syncSource.current === source) {
      return;
    }
    syncScroll(source, nextScrollRow);
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    const source = bitRef.current ?? hexRef.current ?? asciiRef.current;
    if (!source) {
      return;
    }

    let delta = 0;
    switch (event.key) {
      case 'ArrowDown':
        delta = 1;
        break;
      case 'ArrowUp':
        delta = -1;
        break;
      case 'PageDown':
        delta = Math.max(1, Math.floor(VIEWER_HEIGHT / bitSize));
        break;
      case 'PageUp':
        delta = -Math.max(1, Math.floor(VIEWER_HEIGHT / bitSize));
        break;
      case 'Home':
        syncScroll('bit', 0);
        event.preventDefault();
        return;
      case 'End':
        syncScroll('bit', Math.max(0, totalRows - 1));
        event.preventDefault();
        return;
      default:
        return;
    }

    event.preventDefault();
    syncScroll('bit', clamp(scrollRowPosition + delta, 0, Math.max(0, totalRows - 1)));
  }

  const rows = data?.rows ?? [];

  return (
    <section className="viewer-shell" onKeyDown={handleKeyDown} tabIndex={0}>
      <div className="viewer-header">
        <div>Bits</div>
        <div>Hex</div>
        <div>ASCII</div>
      </div>
      <div className="viewer-grid">
        <div className="pane bit-pane" ref={bitRef} onScroll={(event) => handleScroll('bit', event)}>
          <div className="spacer" style={{ height: bitTotalHeight, minWidth: rowWidthBits * bitSize }}>
            {rows.length > 0 ? (
              <div className="rows-layer" style={{ top: bitTopOffset }}>
                <BitCanvas rows={rows} rowWidthBits={rowWidthBits} bitSize={bitSize} />
              </div>
            ) : null}
          </div>
        </div>
        <div className="pane text-pane" ref={hexRef} onScroll={(event) => handleScroll('hex', event)}>
          <div className="spacer" style={{ height: textTotalHeight }}>
            <div className="rows-layer text-rows" style={{ top: textTopOffset }}>
              {rows.map((row) => (
                <div
                  className="text-row"
                  key={`hex-${row.bitOffset}`}
                  style={{ height: textRowHeight, lineHeight: `${textRowHeight}px` }}
                >
                  {renderOffset(row)}
                </div>
              ))}
            </div>
          </div>
        </div>
        <div className="pane text-pane ascii-pane" ref={asciiRef} onScroll={(event) => handleScroll('ascii', event)}>
          <div className="spacer" style={{ height: textTotalHeight }}>
            <div className="rows-layer text-rows" style={{ top: textTopOffset }}>
              {rows.map((row) => (
                <div
                  className="text-row"
                  key={`ascii-${row.bitOffset}`}
                  style={{ height: textRowHeight, lineHeight: `${textRowHeight}px` }}
                >
                  {row.ascii}
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
      {(loading || error) && (
        <div className="viewer-overlay">
          {loading ? 'Loading viewport...' : error}
        </div>
      )}
    </section>
  );
}
