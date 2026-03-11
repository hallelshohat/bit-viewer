import { useEffect, useState } from 'react';

import { createFilterJob, fetchFilterJob, fetchViewMetadata, uploadFile } from './api';
import { FileUpload } from './components/FileUpload';
import { SettingsPanel } from './components/SettingsPanel';
import { Viewer } from './components/Viewer';
import { clamp } from './lib/format';
import type {
  FileMetadata,
  FilterBitRange,
  FilterConfig,
  FilterJobStatus,
  JumpRequest,
  UploadResponse,
  ViewerResource,
  ViewMetadata,
} from './types';

const FILTER_POLL_INTERVAL_MS = 2000;

function toMetadata(upload: UploadResponse): FileMetadata {
  return {
    ...upload,
    createdAt: new Date().toISOString(),
    isFiltered: false,
    sourceFileId: null,
    groupBitLengths: null,
  };
}

function toViewerResourceFromFile(file: FileMetadata): ViewerResource {
  return {
    resourceKind: 'file',
    resourceId: file.fileId,
    filename: file.filename,
    sizeBytes: file.sizeBytes,
    logicalBitLength: file.logicalBitLength,
    createdAt: file.createdAt,
    sourceFileId: file.sourceFileId ?? null,
    groupBitLengths: file.groupBitLengths ?? null,
    isFiltered: false,
  };
}

function toViewerResourceFromView(view: ViewMetadata): ViewerResource {
  return {
    resourceKind: 'view',
    resourceId: view.viewId,
    filename: view.filename,
    sizeBytes: view.sizeBytes,
    logicalBitLength: view.logicalBitLength,
    createdAt: view.createdAt,
    sourceFileId: view.sourceFileId ?? null,
    groupBitLengths: view.groupBitLengths ?? null,
    isFiltered: view.isFiltered,
  };
}

function parseXorMask(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }

  const normalized = trimmed.startsWith('0x') || trimmed.startsWith('0X') ? trimmed.slice(2) : trimmed;
  const parsed = Number.parseInt(normalized, 16);
  if (Number.isNaN(parsed) || parsed < 0 || parsed > 0xff) {
    throw new Error('XOR mask must be a hex byte between 00 and FF.');
  }
  return parsed;
}

function parseRemoveRanges(value: string): FilterBitRange[] {
  const trimmed = value.trim();
  if (!trimmed) {
    return [];
  }

  return trimmed.split(',').map((entry) => {
    const [startPart, lengthPart] = entry.trim().split(':');
    const startBit = Number(startPart);
    const length = Number(lengthPart);
    if (!Number.isInteger(startBit) || startBit < 0 || !Number.isInteger(length) || length <= 0) {
      throw new Error('Remove ranges must use start:length with non-negative integers.');
    }
    return { startBit, length };
  });
}

function hasActiveFilters(config: FilterConfig): boolean {
  return (
    config.invertBits ||
    config.reverseBitsPerByte ||
    config.xorMask !== null ||
    config.preambleBits.length > 0 ||
    config.removeRanges.length > 0
  );
}

export default function App() {
  const [sourceFile, setSourceFile] = useState<FileMetadata | null>(null);
  const [resource, setResource] = useState<ViewerResource | null>(null);
  const [uploading, setUploading] = useState(false);
  const [rowWidthBits, setRowWidthBits] = useState(128);
  const [bitSize, setBitSize] = useState(6);
  const [jumpByteOffset, setJumpByteOffset] = useState('0');
  const [jumpBitOffset, setJumpBitOffset] = useState('0');
  const [jumpRequest, setJumpRequest] = useState<JumpRequest | null>(null);
  const [visibleBitOffset, setVisibleBitOffset] = useState(0);
  const [visibleByteOffset, setVisibleByteOffset] = useState(0);
  const [viewportRows, setViewportRows] = useState(0);
  const [invertBits, setInvertBits] = useState(false);
  const [reverseBitsPerByte, setReverseBitsPerByte] = useState(false);
  const [xorMaskInput, setXorMaskInput] = useState('');
  const [preambleBits, setPreambleBits] = useState('');
  const [removeRangesInput, setRemoveRangesInput] = useState('');
  const [filterJob, setFilterJob] = useState<FilterJobStatus | null>(null);
  const [filterError, setFilterError] = useState<string | null>(null);

  useEffect(() => {
    if (!filterJob?.jobId || (filterJob.status !== 'pending' && filterJob.status !== 'running')) {
      return;
    }

    let cancelled = false;
    let timer: number | null = null;

    const poll = async (jobId: string) => {
      const controller = new AbortController();
      try {
        const nextStatus = await fetchFilterJob(jobId, controller.signal);
        if (cancelled) {
          return;
        }

        if (nextStatus.status === 'completed' && nextStatus.viewId) {
          const viewMetadata = await fetchViewMetadata(nextStatus.viewId, controller.signal);
          if (cancelled) {
            return;
          }
          setResource(toViewerResourceFromView(viewMetadata));
          setFilterJob(nextStatus);
          setFilterError(null);
          return;
        }

        if (nextStatus.status === 'failed') {
          setFilterJob(nextStatus);
          setFilterError(nextStatus.error ?? 'Filter job failed.');
          return;
        }

        setFilterJob(nextStatus);
        timer = window.setTimeout(() => {
          void poll(jobId);
        }, FILTER_POLL_INTERVAL_MS);
      } catch (error) {
        if (!cancelled) {
          setFilterError(error instanceof Error ? error.message : 'Failed to poll filter job.');
        }
      } finally {
        controller.abort();
      }
    };

    timer = window.setTimeout(() => {
      void poll(filterJob.jobId);
    }, FILTER_POLL_INTERVAL_MS);

    return () => {
      cancelled = true;
      if (timer !== null) {
        window.clearTimeout(timer);
      }
    };
  }, [filterJob?.jobId]);

  async function handleUpload(fileToUpload: File) {
    setUploading(true);
    try {
      const response = await uploadFile(fileToUpload);
      const metadata = toMetadata(response);
      setSourceFile(metadata);
      setResource(toViewerResourceFromFile(metadata));
      setVisibleBitOffset(0);
      setVisibleByteOffset(0);
      setJumpByteOffset('0');
      setJumpBitOffset('0');
      setFilterJob(null);
      setFilterError(null);
    } finally {
      setUploading(false);
    }
  }

  async function handleApplyFilters() {
    if (!sourceFile) {
      return;
    }

    try {
      const config: FilterConfig = {
        invertBits,
        reverseBitsPerByte,
        xorMask: parseXorMask(xorMaskInput),
        preambleBits: preambleBits.trim(),
        removeRanges: parseRemoveRanges(removeRangesInput),
      };

      if (!hasActiveFilters(config)) {
        setResource(toViewerResourceFromFile(sourceFile));
        setFilterJob(null);
        setFilterError(null);
        return;
      }

      if (config.removeRanges.length > 0 && !config.preambleBits) {
        throw new Error('Remove ranges require a preamble so the backend can build groups.');
      }

      const response = await createFilterJob(sourceFile.fileId, config);
      setFilterJob({
        jobId: response.jobId,
        sourceFileId: sourceFile.fileId,
        status: 'pending',
        progress: 0,
        viewId: null,
        error: null,
      });
      setFilterError(null);
    } catch (error) {
      setFilterError(error instanceof Error ? error.message : 'Failed to start filter job.');
    }
  }

  function handleUseOriginal() {
    if (!sourceFile) {
      return;
    }
    setResource(toViewerResourceFromFile(sourceFile));
    setFilterJob(null);
    setFilterError(null);
  }

  const filterBusy = filterJob?.status === 'pending' || filterJob?.status === 'running';

  return (
    <main className="app-shell">
      <FileUpload onUpload={handleUpload} uploading={uploading} />
      {resource && sourceFile ? (
        <>
          <SettingsPanel
            resource={resource}
            sourceFile={sourceFile}
            rowWidthBits={rowWidthBits}
            bitSize={bitSize}
            jumpByteOffset={jumpByteOffset}
            jumpBitOffset={jumpBitOffset}
            currentBitOffset={visibleBitOffset}
            currentByteOffset={visibleByteOffset}
            viewportRows={viewportRows}
            invertBits={invertBits}
            reverseBitsPerByte={reverseBitsPerByte}
            xorMaskInput={xorMaskInput}
            preambleBits={preambleBits}
            removeRangesInput={removeRangesInput}
            filterBusy={filterBusy}
            filterProgress={filterJob?.progress ?? 0}
            filterError={filterError}
            onRowWidthBitsChange={(value) =>
              setRowWidthBits(Math.round(clamp(Number.isFinite(value) ? value : 128, 1, 16384)))
            }
            onBitSizeChange={(value) => setBitSize(Math.round(clamp(Number.isFinite(value) ? value : 6, 2, 16)))}
            onJumpByteOffsetChange={setJumpByteOffset}
            onJumpBitOffsetChange={setJumpBitOffset}
            onJump={(request) =>
              setJumpRequest({
                ...request,
                token: Date.now(),
              })
            }
            onInvertBitsChange={setInvertBits}
            onReverseBitsPerByteChange={setReverseBitsPerByte}
            onXorMaskInputChange={setXorMaskInput}
            onPreambleBitsChange={setPreambleBits}
            onRemoveRangesInputChange={setRemoveRangesInput}
            onApplyFilters={handleApplyFilters}
            onUseOriginal={handleUseOriginal}
          />
          <Viewer
            resource={resource}
            rowWidthBits={rowWidthBits}
            bitSize={bitSize}
            jumpRequest={jumpRequest}
            onVisibleOffsetsChange={(bitOffset, byteOffset, rows) => {
              setVisibleBitOffset(bitOffset);
              setVisibleByteOffset(byteOffset);
              setViewportRows(rows);
            }}
          />
        </>
      ) : null}
    </main>
  );
}
