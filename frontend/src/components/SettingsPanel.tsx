import { formatBytes, formatOffset } from '../lib/format';
import type { FileMetadata, JumpRequest } from '../types';

interface SettingsPanelProps {
  file: FileMetadata;
  rowWidthBits: number;
  bitSize: number;
  jumpByteOffset: string;
  jumpBitOffset: string;
  currentBitOffset: number;
  currentByteOffset: number;
  viewportRows: number;
  onRowWidthBitsChange: (value: number) => void;
  onBitSizeChange: (value: number) => void;
  onJumpByteOffsetChange: (value: string) => void;
  onJumpBitOffsetChange: (value: string) => void;
  onJump: (request: Omit<JumpRequest, 'token'>) => void;
}

export function SettingsPanel(props: SettingsPanelProps) {
  return (
    <section className="settings-panel">
      <div className="settings-group">
        <label>
          Row width (bits)
          <input
            type="number"
            min={1}
            max={16384}
            step={1}
            value={props.rowWidthBits}
            onChange={(event) => props.onRowWidthBitsChange(Number(event.target.value))}
          />
        </label>
        <label>
          Bit size (px)
          <input
            type="number"
            min={2}
            max={16}
            value={props.bitSize}
            onChange={(event) => props.onBitSizeChange(Number(event.target.value))}
          />
        </label>
      </div>
      <div className="settings-group">
        <label>
          Jump to byte offset
          <input
            type="number"
            min={0}
            value={props.jumpByteOffset}
            onChange={(event) => props.onJumpByteOffsetChange(event.target.value)}
          />
        </label>
        <button className="secondary-button" type="button" onClick={() => props.onJump({ mode: 'byte', value: Number(props.jumpByteOffset || 0) })}>
          Jump byte
        </button>
      </div>
      <div className="settings-group">
        <label>
          Jump to bit offset
          <input
            type="number"
            min={0}
            value={props.jumpBitOffset}
            onChange={(event) => props.onJumpBitOffsetChange(event.target.value)}
          />
        </label>
        <button className="secondary-button" type="button" onClick={() => props.onJump({ mode: 'bit', value: Number(props.jumpBitOffset || 0) })}>
          Jump bit
        </button>
      </div>
      <div className="stats-bar">
        <span>{props.file.filename}</span>
        <span>{formatBytes(props.file.sizeBytes)}</span>
        <span>Bit {formatOffset(props.currentBitOffset)}</span>
        <span>Byte {formatOffset(props.currentByteOffset)}</span>
        <span>{props.viewportRows} rows loaded</span>
      </div>
    </section>
  );
}
