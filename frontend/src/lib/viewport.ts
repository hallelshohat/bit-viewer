import type { ChunkData, ViewportData, ViewportRow } from '../types';

const CHUNK_SIZE_BYTES = 500 * 1024;
const PRINTABLE_LOW = 32;
const PRINTABLE_HIGH = 126;

interface GroupLayout {
  groupBitLengths: number[];
  groupStartBits: number[];
  groupStartRows: number[];
  totalRows: number;
}

interface RowSourceRange {
  bitOffset: number;
  bitLength: number;
}

export function getChunkSizeBytes(): number {
  return CHUNK_SIZE_BYTES;
}

function buildGroupLayout(groupBitLengths: number[], rowWidthBits: number): GroupLayout {
  const filteredLengths = groupBitLengths.filter((length) => length > 0);
  const groupStartBits: number[] = [];
  const groupStartRows: number[] = [];
  let nextBitOffset = 0;
  let nextRowOffset = 0;

  filteredLengths.forEach((length) => {
    groupStartBits.push(nextBitOffset);
    groupStartRows.push(nextRowOffset);
    nextBitOffset += length;
    nextRowOffset += Math.max(1, Math.ceil(length / rowWidthBits));
  });

  return {
    groupBitLengths: filteredLengths,
    groupStartBits,
    groupStartRows,
    totalRows: nextRowOffset,
  };
}

function findGroupIndex(layout: GroupLayout, rowIndex: number): number {
  let low = 0;
  let high = layout.groupStartRows.length - 1;
  let result = 0;

  while (low <= high) {
    const mid = Math.floor((low + high) / 2);
    if (layout.groupStartRows[mid] <= rowIndex) {
      result = mid;
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }

  return result;
}

function getRowSourceRange(params: {
  logicalBitLength: number;
  rowWidthBits: number;
  rowIndex: number;
  groupBitLengths?: number[];
}): RowSourceRange | null {
  if (params.logicalBitLength <= 0) {
    return null;
  }

  if (!params.groupBitLengths || params.groupBitLengths.length === 0) {
    const startBit = params.rowIndex * params.rowWidthBits;
    if (startBit >= params.logicalBitLength) {
      return null;
    }
    const endBit = Math.min(params.logicalBitLength, startBit + params.rowWidthBits);
    return {
      bitOffset: startBit,
      bitLength: endBit - startBit,
    };
  }

  const layout = buildGroupLayout(params.groupBitLengths, params.rowWidthBits);
  if (params.rowIndex < 0 || params.rowIndex >= layout.totalRows) {
    return null;
  }

  const groupIndex = findGroupIndex(layout, params.rowIndex);
  const inGroupRow = params.rowIndex - layout.groupStartRows[groupIndex];
  const groupBitLength = layout.groupBitLengths[groupIndex];
  const rowStartInGroup = inGroupRow * params.rowWidthBits;
  const bitLength = Math.min(params.rowWidthBits, groupBitLength - rowStartInGroup);

  return {
    bitOffset: layout.groupStartBits[groupIndex] + rowStartInGroup,
    bitLength,
  };
}

export function getRowSourceBitOffset(params: {
  logicalBitLength: number;
  rowWidthBits: number;
  rowIndex: number;
  groupBitLengths?: number[];
}): number {
  return (
    getRowSourceRange({
      logicalBitLength: params.logicalBitLength,
      rowWidthBits: params.rowWidthBits,
      rowIndex: params.rowIndex,
      groupBitLengths: params.groupBitLengths,
    })?.bitOffset ?? 0
  );
}

export function getRowIndexForBitOffset(params: {
  bitOffset: number;
  rowWidthBits: number;
  groupBitLengths?: number[];
}): number {
  if (!params.groupBitLengths || params.groupBitLengths.length === 0) {
    return Math.floor(params.bitOffset / params.rowWidthBits);
  }

  const layout = buildGroupLayout(params.groupBitLengths, params.rowWidthBits);
  let low = 0;
  let high = layout.groupStartBits.length - 1;
  let result = 0;

  while (low <= high) {
    const mid = Math.floor((low + high) / 2);
    if (layout.groupStartBits[mid] <= params.bitOffset) {
      result = mid;
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }

  return layout.groupStartRows[result] + Math.floor((params.bitOffset - layout.groupStartBits[result]) / params.rowWidthBits);
}

export function getTotalRows(params: {
  logicalBitLength: number;
  rowWidthBits: number;
  groupBitLengths?: number[];
}): number {
  if (params.logicalBitLength <= 0) {
    return 0;
  }

  if (!params.groupBitLengths || params.groupBitLengths.length === 0) {
    return Math.ceil(params.logicalBitLength / params.rowWidthBits);
  }

  return buildGroupLayout(params.groupBitLengths, params.rowWidthBits).totalRows;
}

export function getRequiredChunkOffsets(params: {
  fileSizeBytes: number;
  logicalBitLength: number;
  startRow: number;
  visibleRows: number;
  rowWidthBits: number;
  groupBitLengths?: number[];
}): number[] {
  const totalRows = getTotalRows({
    logicalBitLength: params.logicalBitLength,
    rowWidthBits: params.rowWidthBits,
    groupBitLengths: params.groupBitLengths,
  });

  if (totalRows === 0) {
    return [0];
  }

  const startRange = getRowSourceRange({
    logicalBitLength: params.logicalBitLength,
    rowWidthBits: params.rowWidthBits,
    rowIndex: params.startRow,
    groupBitLengths: params.groupBitLengths,
  });
  const endRange = getRowSourceRange({
    logicalBitLength: params.logicalBitLength,
    rowWidthBits: params.rowWidthBits,
    rowIndex: Math.min(totalRows - 1, params.startRow + params.visibleRows - 1),
    groupBitLengths: params.groupBitLengths,
  });

  if (!startRange || !endRange) {
    return [0];
  }

  const startByte = Math.floor(startRange.bitOffset / 8);
  const endByte = Math.ceil((endRange.bitOffset + endRange.bitLength) / 8);
  const firstChunkOffset = Math.floor(startByte / CHUNK_SIZE_BYTES) * CHUNK_SIZE_BYTES;
  const offsets: number[] = [];

  for (let chunkOffset = firstChunkOffset; chunkOffset < endByte; chunkOffset += CHUNK_SIZE_BYTES) {
    offsets.push(chunkOffset);
  }

  if (offsets.length === 0) {
    offsets.push(Math.floor(Math.min(startByte, params.fileSizeBytes) / CHUNK_SIZE_BYTES) * CHUNK_SIZE_BYTES);
  }

  return offsets;
}

function extractBits(data: Uint8Array, baseByteOffset: number, bitOffset: number, bitLength: number): string {
  const parts: string[] = [];
  for (let absoluteBit = bitOffset; absoluteBit < bitOffset + bitLength; absoluteBit += 1) {
    const absoluteByte = Math.floor(absoluteBit / 8);
    const relativeByte = absoluteByte - baseByteOffset;
    const bitInByte = 7 - (absoluteBit % 8);
    const value = (data[relativeByte] >> bitInByte) & 1;
    parts.push(value === 1 ? '1' : '0');
  }
  return parts.join('');
}

function bitsToHexAscii(bits: string): { hex: string; ascii: string } {
  if (bits.length === 0) {
    return { hex: '', ascii: '' };
  }

  const hexParts: string[] = [];
  const asciiParts: string[] = [];

  for (let offset = 0; offset < bits.length; offset += 8) {
    const chunk = bits.slice(offset, offset + 8).padEnd(8, '0');
    const value = Number.parseInt(chunk, 2);
    hexParts.push(value.toString(16).toUpperCase().padStart(2, '0'));
    asciiParts.push(value >= PRINTABLE_LOW && value <= PRINTABLE_HIGH ? String.fromCharCode(value) : '.');
  }

  return {
    hex: hexParts.join(' '),
    ascii: asciiParts.join(''),
  };
}

function bytesToHexAscii(data: Uint8Array): { hex: string; ascii: string } {
  return {
    hex: Array.from(data, (value) => value.toString(16).toUpperCase().padStart(2, '0')).join(' '),
    ascii: Array.from(data, (value) => (value >= PRINTABLE_LOW && value <= PRINTABLE_HIGH ? String.fromCharCode(value) : '.')).join(''),
  };
}

function readByteRange(chunks: Map<number, ChunkData>, byteOffsetStart: number, byteOffsetEnd: number): Uint8Array | null {
  const length = byteOffsetEnd - byteOffsetStart;
  const result = new Uint8Array(length);
  let writeOffset = 0;
  let cursor = byteOffsetStart;

  while (cursor < byteOffsetEnd) {
    const chunkOffset = Math.floor(cursor / CHUNK_SIZE_BYTES) * CHUNK_SIZE_BYTES;
    const chunk = chunks.get(chunkOffset);
    if (!chunk) {
      return null;
    }

    const chunkRelativeStart = cursor - chunk.byteOffset;
    if (chunkRelativeStart < 0 || chunkRelativeStart >= chunk.byteLength) {
      return null;
    }

    const bytesAvailable = Math.min(chunk.byteLength - chunkRelativeStart, byteOffsetEnd - cursor);
    result.set(chunk.data.subarray(chunkRelativeStart, chunkRelativeStart + bytesAvailable), writeOffset);
    cursor += bytesAvailable;
    writeOffset += bytesAvailable;
  }

  return result;
}

export function buildViewportData(params: {
  fileSizeBytes: number;
  logicalBitLength: number;
  startRow: number;
  visibleRows: number;
  rowWidthBits: number;
  chunks: Map<number, ChunkData>;
  groupBitLengths?: number[];
  useLogicalRowBytes?: boolean;
}): ViewportData {
  const rows: ViewportRow[] = [];
  const totalRows = getTotalRows({
    logicalBitLength: params.logicalBitLength,
    rowWidthBits: params.rowWidthBits,
    groupBitLengths: params.groupBitLengths,
  });

  for (let rowIndex = 0; rowIndex < params.visibleRows; rowIndex += 1) {
    const absoluteRowIndex = params.startRow + rowIndex;
    if (absoluteRowIndex >= totalRows) {
      break;
    }

    const rowSourceRange = getRowSourceRange({
      logicalBitLength: params.logicalBitLength,
      rowWidthBits: params.rowWidthBits,
      rowIndex: absoluteRowIndex,
      groupBitLengths: params.groupBitLengths,
    });
    if (!rowSourceRange) {
      break;
    }

    const rowEndBit = rowSourceRange.bitOffset + rowSourceRange.bitLength;
    const byteOffsetStart = Math.floor(rowSourceRange.bitOffset / 8);
    const byteOffsetEnd = Math.ceil(rowEndBit / 8);
    const rowBytes = readByteRange(params.chunks, byteOffsetStart, byteOffsetEnd);
    if (!rowBytes) {
      break;
    }

    const bits = extractBits(rowBytes, byteOffsetStart, rowSourceRange.bitOffset, rowSourceRange.bitLength);
    const logicalBytes = params.useLogicalRowBytes || Boolean(params.groupBitLengths && params.groupBitLengths.length > 0);
    const { hex, ascii } = logicalBytes ? bitsToHexAscii(bits) : bytesToHexAscii(rowBytes);

    rows.push({
      rowIndex: absoluteRowIndex,
      bitOffset: rowSourceRange.bitOffset,
      bitLength: rowSourceRange.bitLength,
      byteOffsetStart,
      byteOffsetEnd,
      bits,
      hex,
      ascii,
    });
  }

  return {
    requestedBitOffset: rows[0]?.bitOffset ?? 0,
    rowWidthBits: params.rowWidthBits,
    actualRows: rows.length,
    rows,
  };
}
