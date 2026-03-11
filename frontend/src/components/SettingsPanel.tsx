import { formatBytes, formatOffset } from '../lib/format';
import type { FileMetadata, JumpRequest, ViewerResource } from '../types';

interface SettingsPanelProps {
  resource: ViewerResource;
  sourceFile: FileMetadata;
  rowWidthBits: number;
  bitSize: number;
  jumpByteOffset: string;
  jumpBitOffset: string;
  currentBitOffset: number;
  currentByteOffset: number;
  viewportRows: number;
  invertBits: boolean;
  reverseBitsPerByte: boolean;
  xorMaskInput: string;
  preambleBits: string;
  removeRangesInput: string;
  filterBusy: boolean;
  filterProgress: number;
  filterError: string | null;
  onRowWidthBitsChange: (value: number) => void;
  onBitSizeChange: (value: number) => void;
  onJumpByteOffsetChange: (value: string) => void;
  onJumpBitOffsetChange: (value: string) => void;
  onJump: (request: Omit<JumpRequest, 'token'>) => void;
  onInvertBitsChange: (value: boolean) => void;
  onReverseBitsPerByteChange: (value: boolean) => void;
  onXorMaskInputChange: (value: string) => void;
  onPreambleBitsChange: (value: string) => void;
  onRemoveRangesInputChange: (value: string) => void;
  onApplyFilters: () => void;
  onUseOriginal: () => void;
}

export function SettingsPanel(props: SettingsPanelProps) {
  const viewingFiltered = props.resource.resourceKind === 'view';

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
      <div className="filter-panel">
        <div className="filter-heading">
          <div>
            <strong>Filters</strong>
            <div className="filter-hint">Backend preprocessing writes a derived view, then the viewer streams it in chunks.</div>
          </div>
          <div className="filter-actions">
            <button className="secondary-button" type="button" onClick={props.onUseOriginal} disabled={props.filterBusy || !viewingFiltered}>
              Use original
            </button>
            <button className="primary-button" type="button" onClick={props.onApplyFilters} disabled={props.filterBusy}>
              {props.filterBusy ? 'Applying...' : 'Apply filters'}
            </button>
          </div>
        </div>
        <div className="settings-group checkbox-group">
          <label className="checkbox-field">
            <input type="checkbox" checked={props.invertBits} onChange={(event) => props.onInvertBitsChange(event.target.checked)} />
            <span>Invert bits</span>
          </label>
          <label className="checkbox-field">
            <input
              type="checkbox"
              checked={props.reverseBitsPerByte}
              onChange={(event) => props.onReverseBitsPerByteChange(event.target.checked)}
            />
            <span>Reverse every byte</span>
          </label>
          <label>
            XOR mask (hex)
            <input
              type="text"
              placeholder="FF"
              value={props.xorMaskInput}
              onChange={(event) => props.onXorMaskInputChange(event.target.value)}
            />
          </label>
        </div>
        <div className="settings-group">
          <label className="wide-field">
            Group preamble bits
            <input
              type="text"
              inputMode="numeric"
              placeholder="10110011"
              value={props.preambleBits}
              onChange={(event) => props.onPreambleBitsChange(event.target.value.replace(/[^01]/g, ''))}
            />
          </label>
        </div>
        <div className="settings-group">
          <label className="wide-field">
            Remove ranges per group
            <input
              type="text"
              placeholder="6:3, 18:4"
              value={props.removeRangesInput}
              onChange={(event) => props.onRemoveRangesInputChange(event.target.value)}
            />
          </label>
        </div>
        <div className="filter-hint">Range format is `start:length`, comma separated, relative to each group start.</div>
        {props.filterBusy ? <div className="filter-progress">Processing filtered view: {props.filterProgress}%</div> : null}
        {props.filterError ? <div className="error-banner">{props.filterError}</div> : null}
      </div>
      <div className="stats-bar">
        <span>Source {props.sourceFile.filename}</span>
        <span>Viewing {viewingFiltered ? 'filtered view' : 'original file'}</span>
        <span>{formatBytes(props.resource.sizeBytes)}</span>
        <span>Bits {formatOffset(props.resource.logicalBitLength)}</span>
        <span>Bit {formatOffset(props.currentBitOffset)}</span>
        <span>Byte {formatOffset(props.currentByteOffset)}</span>
        <span>{props.viewportRows} rows loaded</span>
      </div>
    </section>
  );
}
