export interface UploadResponse {
  fileId: string;
  filename: string;
  sizeBytes: number;
}

export interface FileMetadata extends UploadResponse {
  createdAt: string;
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
  fileId: string;
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
