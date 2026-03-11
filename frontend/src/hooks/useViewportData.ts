import { useEffect, useMemo, useRef, useState } from 'react';

import { fetchChunk } from '../api';
import { LruCache } from '../lib/lru';
import { buildViewportData, getChunkSizeBytes, getRequiredChunkOffsets } from '../lib/viewport';
import type { ChunkData } from '../types';

const chunkCache = new LruCache<string, ChunkData>(32);
const inflightRequests = new Map<string, Promise<ChunkData>>();
const CHUNK_SIZE_BYTES = getChunkSizeBytes();
const MAIN_REQUEST_DEBOUNCE_MS = 40;
const PREFETCH_DEBOUNCE_MS = 120;

function makeKey(resourceKind: string, resourceId: string, byteOffset: number): string {
  return `${resourceKind}:${resourceId}:${byteOffset}:${CHUNK_SIZE_BYTES}`;
}

async function loadChunk(
  resourceKind: string,
  resourceId: string,
  byteOffset: number,
  signal?: AbortSignal,
): Promise<ChunkData> {
  const key = makeKey(resourceKind, resourceId, byteOffset);
  const cached = chunkCache.get(key);
  if (cached) {
    return cached;
  }

  const existing = inflightRequests.get(key);
  if (existing) {
    return existing;
  }

  const promise = fetchChunk({ resourceId, resourceKind: resourceKind as 'file' | 'view', byteOffset, byteLength: CHUNK_SIZE_BYTES, signal })
    .then((chunk) => {
      chunkCache.set(key, chunk);
      inflightRequests.delete(key);
      return chunk;
    })
    .catch((error) => {
      inflightRequests.delete(key);
      throw error;
    });

  inflightRequests.set(key, promise);
  return promise;
}

export function useViewportData(params: {
  resourceId: string;
  resourceKind: 'file' | 'view';
  fileSizeBytes: number;
  logicalBitLength: number;
  startRow: number;
  visibleRows: number;
  rowWidthBits: number;
  groupBitLengths?: number[] | null;
  useLogicalRowBytes?: boolean;
}) {
  const [revision, setRevision] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestId = useRef(0);

  const requiredChunkOffsets = useMemo(
    () =>
      getRequiredChunkOffsets({
        fileSizeBytes: params.fileSizeBytes,
        logicalBitLength: params.logicalBitLength,
        startRow: params.startRow,
        visibleRows: params.visibleRows,
        rowWidthBits: params.rowWidthBits,
        groupBitLengths: params.groupBitLengths ?? undefined,
      }),
    [params.fileSizeBytes, params.groupBitLengths, params.logicalBitLength, params.rowWidthBits, params.startRow, params.visibleRows],
  );
  const chunkMap = useMemo(() => {
    const map = new Map<number, ChunkData>();
    requiredChunkOffsets.forEach((offset) => {
      const cached = chunkCache.get(makeKey(params.resourceKind, params.resourceId, offset));
      if (cached) {
        map.set(offset, cached);
      }
    });
    return map;
  }, [params.resourceId, params.resourceKind, requiredChunkOffsets, revision]);
  const missingChunkOffsets = useMemo(
    () => requiredChunkOffsets.filter((offset) => !chunkMap.has(offset)),
    [chunkMap, requiredChunkOffsets],
  );

  const data = useMemo(
    () =>
      buildViewportData({
        fileSizeBytes: params.fileSizeBytes,
        logicalBitLength: params.logicalBitLength,
        startRow: params.startRow,
        visibleRows: params.visibleRows,
        rowWidthBits: params.rowWidthBits,
        chunks: chunkMap,
        groupBitLengths: params.groupBitLengths ?? undefined,
        useLogicalRowBytes: params.useLogicalRowBytes ?? false,
      }),
    [
      chunkMap,
      params.fileSizeBytes,
      params.groupBitLengths,
      params.logicalBitLength,
      params.rowWidthBits,
      params.startRow,
      params.useLogicalRowBytes,
      params.visibleRows,
    ],
  );

  useEffect(() => {
    if (missingChunkOffsets.length === 0) {
      setLoading(false);
      return;
    }

    const controller = new AbortController();
    const currentRequest = requestId.current + 1;
    requestId.current = currentRequest;
    setLoading(true);
    setError(null);

    const requestTimer = window.setTimeout(() => {
      Promise.all(
        missingChunkOffsets.map((offset) =>
          loadChunk(params.resourceKind, params.resourceId, offset, controller.signal),
        ),
      )
        .then(() => {
          if (requestId.current !== currentRequest) {
            return;
          }
          setRevision((value) => value + 1);
          setLoading(false);
        })
        .catch((err: unknown) => {
          if (controller.signal.aborted || requestId.current !== currentRequest) {
            return;
          }
          setError(err instanceof Error ? err.message : 'Unknown chunk error');
          setLoading(false);
        });
    }, MAIN_REQUEST_DEBOUNCE_MS);

    return () => {
      controller.abort();
      window.clearTimeout(requestTimer);
    };
  }, [missingChunkOffsets, params.resourceId, params.resourceKind]);

  useEffect(() => {
    if (requiredChunkOffsets.length === 0) {
      return;
    }

    const previousOffset = requiredChunkOffsets[0] - CHUNK_SIZE_BYTES;
    const nextOffset = requiredChunkOffsets[requiredChunkOffsets.length - 1] + CHUNK_SIZE_BYTES;
    const prefetchOffsets = [previousOffset, nextOffset].filter(
      (offset) => offset >= 0 && offset < params.fileSizeBytes,
    );

    const prefetchTimer = window.setTimeout(() => {
      prefetchOffsets.forEach((offset) => {
        void loadChunk(params.resourceKind, params.resourceId, offset).catch(() => undefined);
      });
    }, PREFETCH_DEBOUNCE_MS);

    return () => {
      window.clearTimeout(prefetchTimer);
    };
  }, [params.fileSizeBytes, params.resourceId, params.resourceKind, requiredChunkOffsets]);

  return { data, loading, error };
}
