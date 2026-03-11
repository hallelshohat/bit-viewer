import type {
  ChunkData,
  FilterConfig,
  FilterJobResponse,
  FilterJobStatus,
  ResourceKind,
  UploadResponse,
  ViewMetadata,
  ViewportResponse,
} from './types';

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
  resourceId: string;
  resourceKind: ResourceKind;
  byteOffset: number;
  byteLength: number;
  signal?: AbortSignal;
}): Promise<ChunkData> {
  const query = new URLSearchParams({
    byteOffset: String(params.byteOffset),
    byteLength: String(params.byteLength),
  });
  const prefix = params.resourceKind === 'file' ? 'files' : 'views';
  const response = await fetch(`${API_BASE}/api/${prefix}/${params.resourceId}/chunk?${query.toString()}`, {
    signal: params.signal,
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || `Request failed: ${response.status}`);
  }

  const arrayBuffer = await response.arrayBuffer();
  return {
    resourceId: params.resourceId,
    resourceKind: params.resourceKind,
    byteOffset: Number(response.headers.get('X-Byte-Offset') ?? params.byteOffset),
    byteLength: arrayBuffer.byteLength,
    data: new Uint8Array(arrayBuffer),
  };
}

export async function createFilterJob(fileId: string, config: FilterConfig): Promise<FilterJobResponse> {
  const response = await fetch(`${API_BASE}/api/files/${fileId}/filters`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(config),
  });
  return parseJson<FilterJobResponse>(response);
}

export async function fetchFilterJob(jobId: string, signal?: AbortSignal): Promise<FilterJobStatus> {
  const response = await fetch(`${API_BASE}/api/filter-jobs/${jobId}`, {
    signal,
  });
  return parseJson<FilterJobStatus>(response);
}

export async function fetchViewMetadata(viewId: string, signal?: AbortSignal): Promise<ViewMetadata> {
  const response = await fetch(`${API_BASE}/api/views/${viewId}/metadata`, {
    signal,
  });
  return parseJson<ViewMetadata>(response);
}
