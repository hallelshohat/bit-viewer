import { useEffect, useRef } from 'react';

import type { ViewportRow } from '../types';

interface BitCanvasProps {
  rows: ViewportRow[];
  rowWidthBits: number;
  bitSize: number;
}

const ZERO_COLOR = [255, 255, 255, 255];
const ONE_COLOR = [37, 99, 235, 255];

export function BitCanvas({ rows, rowWidthBits, bitSize }: BitCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }

    const width = rowWidthBits;
    const height = Math.max(rows.length, 1);
    // The canvas stores one pixel per bit and relies on CSS scaling for the
    // visible square size. That avoids creating very large draw buffers.
    canvas.width = width;
    canvas.height = height;
    canvas.style.width = `${rowWidthBits * bitSize}px`;
    canvas.style.height = `${height * bitSize}px`;

    const context = canvas.getContext('2d');
    if (!context) {
      return;
    }

    const image = context.createImageData(width, height);
    const pixels = image.data;

    rows.forEach((row, rowIndex) => {
      for (let column = 0; column < rowWidthBits; column += 1) {
        const bit = row.bits[column] ?? '0';
        const pixelOffset = (rowIndex * width + column) * 4;
        const color = bit === '1' ? ONE_COLOR : ZERO_COLOR;
        pixels[pixelOffset] = color[0];
        pixels[pixelOffset + 1] = color[1];
        pixels[pixelOffset + 2] = color[2];
        pixels[pixelOffset + 3] = color[3];
      }
    });

    context.putImageData(image, 0, 0);
  }, [bitSize, rowWidthBits, rows]);

  return <canvas className="bit-canvas" ref={canvasRef} />;
}
