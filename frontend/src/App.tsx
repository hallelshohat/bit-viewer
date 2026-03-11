import { useState } from 'react';

import { uploadFile } from './api';
import { FileUpload } from './components/FileUpload';
import { SettingsPanel } from './components/SettingsPanel';
import { Viewer } from './components/Viewer';
import { clamp } from './lib/format';
import type { FileMetadata, JumpRequest, UploadResponse } from './types';

function toMetadata(upload: UploadResponse): FileMetadata {
  return {
    ...upload,
    createdAt: new Date().toISOString(),
  };
}

export default function App() {
  const [file, setFile] = useState<FileMetadata | null>(null);
  const [uploading, setUploading] = useState(false);
  const [rowWidthBits, setRowWidthBits] = useState(128);
  const [bitSize, setBitSize] = useState(6);
  const [jumpByteOffset, setJumpByteOffset] = useState('0');
  const [jumpBitOffset, setJumpBitOffset] = useState('0');
  const [jumpRequest, setJumpRequest] = useState<JumpRequest | null>(null);
  const [visibleBitOffset, setVisibleBitOffset] = useState(0);
  const [visibleByteOffset, setVisibleByteOffset] = useState(0);
  const [viewportRows, setViewportRows] = useState(0);

  async function handleUpload(fileToUpload: File) {
    setUploading(true);
    try {
      const response = await uploadFile(fileToUpload);
      setFile(toMetadata(response));
      setVisibleBitOffset(0);
      setVisibleByteOffset(0);
      setJumpByteOffset('0');
      setJumpBitOffset('0');
    } finally {
      setUploading(false);
    }
  }

  return (
    <main className="app-shell">
      <FileUpload onUpload={handleUpload} uploading={uploading} />
      {file ? (
        <>
          <SettingsPanel
            file={file}
            rowWidthBits={rowWidthBits}
            bitSize={bitSize}
            jumpByteOffset={jumpByteOffset}
            jumpBitOffset={jumpBitOffset}
            currentBitOffset={visibleBitOffset}
            currentByteOffset={visibleByteOffset}
            viewportRows={viewportRows}
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
          />
          <Viewer
            file={file}
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
