import type { ChunkData, UploadResponse, ViewportResponse } from './types';

const API_BASE = import.meta.env.VITE_API_BASE_URL ?? '';

async function parseJson<T>(response: Response): Promise<T> {
  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || `Request failed: ${response.status}`);
  }
  return response.json() as Promise<T>;
}

export async function uploadFile(file: File): Promise<UploadResponse> {
  const formData = new FormData();
  formData.append('file', file);
  const response = await fetch(`${API_BASE}/api/files/upload`, {
    method: 'POST',
    body: formData,
  });
  return parseJson<UploadResponse>(response);
}

export async function fetchViewport(params: {
  fileId: string;
  bitOffset: number;
  visibleRows: number;
  rowWidthBits: number;
  signal?: AbortSignal;
}): Promise<ViewportResponse> {
  const query = new URLSearchParams({
    bitOffset: String(params.bitOffset),
    visibleRows: String(params.visibleRows),
    rowWidthBits: String(params.rowWidthBits),
  });
  const response = await fetch(`${API_BASE}/api/files/${params.fileId}/viewport?${query.toString()}`, {
    signal: params.signal,
  });
  return parseJson<ViewportResponse>(response);
}

export async function fetchChunk(params: {
  fileId: string;
  byteOffset: number;
  byteLength: number;
  signal?: AbortSignal;
}): Promise<ChunkData> {
  const query = new URLSearchParams({
    byteOffset: String(params.byteOffset),
    byteLength: String(params.byteLength),
  });
  const response = await fetch(`${API_BASE}/api/files/${params.fileId}/chunk?${query.toString()}`, {
    signal: params.signal,
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || `Request failed: ${response.status}`);
  }

  const arrayBuffer = await response.arrayBuffer();
  return {
    fileId: params.fileId,
    byteOffset: Number(response.headers.get('X-Byte-Offset') ?? params.byteOffset),
    byteLength: arrayBuffer.byteLength,
    data: new Uint8Array(arrayBuffer),
  };
}
