import type { ChunkData, ViewportData, ViewportRow } from '../types';

const CHUNK_SIZE_BYTES = 500 * 1024;
const PRINTABLE_LOW = 32;
const PRINTABLE_HIGH = 126;

export function getChunkSizeBytes(): number {
  return CHUNK_SIZE_BYTES;
}

export function getRequiredChunkOffsets(params: {
  fileSizeBytes: number;
  startRow: number;
  visibleRows: number;
  rowWidthBits: number;
}): number[] {
  const totalBits = params.fileSizeBytes * 8;
  const startBit = Math.min(totalBits, params.startRow * params.rowWidthBits);
  const endBit = Math.min(totalBits, (params.startRow + params.visibleRows) * params.rowWidthBits);
  const startByte = Math.floor(startBit / 8);
  const endByte = Math.ceil(endBit / 8);
  const firstChunkOffset = Math.floor(startByte / CHUNK_SIZE_BYTES) * CHUNK_SIZE_BYTES;
  const offsets: number[] = [];

  for (let chunkOffset = firstChunkOffset; chunkOffset < endByte; chunkOffset += CHUNK_SIZE_BYTES) {
    offsets.push(chunkOffset);
  }

  if (offsets.length === 0) {
    offsets.push(firstChunkOffset);
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
  startRow: number;
  visibleRows: number;
  rowWidthBits: number;
  chunks: Map<number, ChunkData>;
}): ViewportData {
  const totalBits = params.fileSizeBytes * 8;
  const requestedBitOffset = params.startRow * params.rowWidthBits;
  const rows: ViewportRow[] = [];

  for (let rowIndex = 0; rowIndex < params.visibleRows; rowIndex += 1) {
    const rowBitOffset = requestedBitOffset + rowIndex * params.rowWidthBits;
    if (rowBitOffset >= totalBits) {
      break;
    }

    const rowEndBit = Math.min(totalBits, rowBitOffset + params.rowWidthBits);
    const rowBitLength = rowEndBit - rowBitOffset;
    const byteOffsetStart = Math.floor(rowBitOffset / 8);
    const byteOffsetEnd = Math.ceil(rowEndBit / 8);
    const rowBytes = readByteRange(params.chunks, byteOffsetStart, byteOffsetEnd);
    if (!rowBytes) {
      break;
    }

    const { hex, ascii } = bytesToHexAscii(rowBytes);
    rows.push({
      rowIndex,
      bitOffset: rowBitOffset,
      bitLength: rowBitLength,
      byteOffsetStart,
      byteOffsetEnd,
      bits: extractBits(rowBytes, byteOffsetStart, rowBitOffset, rowBitLength),
      hex,
      ascii,
    });
  }

  return {
    requestedBitOffset,
    rowWidthBits: params.rowWidthBits,
    actualRows: rows.length,
    rows,
  };
}
