export interface UploadResponse {
  fileId: string;
  filename: string;
  sizeBytes: number;
  logicalBitLength: number;
}

export interface FileMetadata extends UploadResponse {
  createdAt: string;
  sourceFileId?: string | null;
  groupBitLengths?: number[] | null;
  isFiltered?: boolean;
}

export interface ViewMetadata {
  viewId: string;
  filename: string;
  sizeBytes: number;
  logicalBitLength: number;
  createdAt: string;
  sourceFileId?: string | null;
  groupBitLengths?: number[] | null;
  isFiltered: boolean;
}

export type ResourceKind = 'file' | 'view';

export interface ViewerResource {
  resourceKind: ResourceKind;
  resourceId: string;
  filename: string;
  sizeBytes: number;
  logicalBitLength: number;
  createdAt: string;
  sourceFileId?: string | null;
  groupBitLengths?: number[] | null;
  isFiltered: boolean;
}

export interface ViewportRow {
  rowIndex: number;
  bitOffset: number;
  bitLength: number;
  byteOffsetStart: number;
  byteOffsetEnd: number;
  bits: string;
  hex: string;
  ascii: string;
}

export interface ViewportResponse {
  fileId: string;
  filename: string;
  sizeBytes: number;
  requestedBitOffset: number;
  visibleRows: number;
  rowWidthBits: number;
  actualRows: number;
  bitRange: {
    start: number;
    end: number;
    length: number;
  };
  byteRange: {
    start: number;
    end: number;
    length: number;
  };
  rows: ViewportRow[];
}

export interface ChunkData {
  resourceId: string;
  resourceKind: ResourceKind;
  byteOffset: number;
  byteLength: number;
  data: Uint8Array;
}

export interface ViewportData {
  requestedBitOffset: number;
  rowWidthBits: number;
  actualRows: number;
  rows: ViewportRow[];
}

export interface JumpRequest {
  mode: 'bit' | 'byte';
  value: number;
  token: number;
}

export interface FilterBitRange {
  startBit: number;
  length: number;
}

export interface FilterConfig {
  invertBits: boolean;
  reverseBitsPerByte: boolean;
  xorMask: number | null;
  preambleBits: string;
  removeRanges: FilterBitRange[];
}

export interface FilterJobResponse {
  jobId: string;
}

export interface FilterJobStatus {
  jobId: string;
  sourceFileId: string;
  status: 'pending' | 'running' | 'completed' | 'failed';
  progress: number;
  viewId?: string | null;
  error?: string | null;
}
