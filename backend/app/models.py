from datetime import datetime
from typing import List

from pydantic import BaseModel, Field


class FileMetadata(BaseModel):
    file_id: str = Field(alias="fileId")
    filename: str
    size_bytes: int = Field(alias="sizeBytes")
    created_at: datetime = Field(alias="createdAt")

    class Config:
        allow_population_by_field_name = True


class UploadResponse(BaseModel):
    file_id: str = Field(alias="fileId")
    filename: str
    size_bytes: int = Field(alias="sizeBytes")

    class Config:
        allow_population_by_field_name = True


class ByteRange(BaseModel):
    start: int
    end: int
    length: int


class BitRange(BaseModel):
    start: int
    end: int
    length: int


class ViewportRow(BaseModel):
    row_index: int = Field(alias="rowIndex")
    bit_offset: int = Field(alias="bitOffset")
    bit_length: int = Field(alias="bitLength")
    byte_offset_start: int = Field(alias="byteOffsetStart")
    byte_offset_end: int = Field(alias="byteOffsetEnd")
    bits: str
    hex: str
    ascii: str

    class Config:
        allow_population_by_field_name = True


class ViewportResponse(BaseModel):
    file_id: str = Field(alias="fileId")
    filename: str
    size_bytes: int = Field(alias="sizeBytes")
    requested_bit_offset: int = Field(alias="requestedBitOffset")
    visible_rows: int = Field(alias="visibleRows")
    row_width_bits: int = Field(alias="rowWidthBits")
    actual_rows: int = Field(alias="actualRows")
    bit_range: BitRange = Field(alias="bitRange")
    byte_range: ByteRange = Field(alias="byteRange")
    rows: List[ViewportRow]

    class Config:
        allow_population_by_field_name = True
