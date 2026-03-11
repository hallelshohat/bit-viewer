from __future__ import annotations

import logging
import time
from concurrent.futures import ThreadPoolExecutor
from threading import Lock
from typing import Dict

from .filtering import process_filtered_view
from .models import (
    CreateFilterJobResponse,
    FilterConfig,
    FilterJobStatusResponse,
    ViewMetadata,
)
from .storage import FileStore

logger = logging.getLogger(__name__)


class FilterJobManager:
    def __init__(self, store: FileStore) -> None:
        self.store = store
        self.executor = ThreadPoolExecutor(max_workers=2, thread_name_prefix="bit-viewer-filters")
        self.lock = Lock()
        self.jobs: Dict[str, FilterJobStatusResponse] = {}
        self.logged_progress: Dict[str, int] = {}

    def create_job(self, file_id: str, config: FilterConfig) -> CreateFilterJobResponse:
        self.store.get_metadata(file_id)
        job_id, output_path = self.store.create_view()
        status = FilterJobStatusResponse(
            jobId=job_id,
            sourceFileId=file_id,
            status="pending",
            progress=0,
        )
        with self.lock:
            self.jobs[job_id] = status
            self.logged_progress[job_id] = 0

        logger.info(
            "created filter job job_id=%s source_file_id=%s invert=%s reverse_bits_per_byte=%s xor_mask=%s preamble_bits=%s remove_ranges=%d",
            job_id,
            file_id,
            config.invert_bits,
            config.reverse_bits_per_byte,
            config.xor_mask,
            len(config.preamble_bits or ""),
            len(config.remove_ranges),
        )

        self.executor.submit(self._run_job, job_id, file_id, output_path, config)
        return CreateFilterJobResponse(jobId=job_id)

    def get_job(self, job_id: str) -> FilterJobStatusResponse:
        with self.lock:
            status = self.jobs.get(job_id)
        if status is None:
            raise FileNotFoundError(job_id)
        return status

    def _set_job_status(self, job_id: str, **updates) -> None:
        with self.lock:
            current = self.jobs[job_id]
            self.jobs[job_id] = current.copy(update=updates)

    def _update_progress(self, job_id: str, progress: int) -> None:
        self._set_job_status(job_id, status="running", progress=progress)

        should_log = False
        with self.lock:
            last_logged = self.logged_progress.get(job_id, 0)
            if progress >= min(100, last_logged + 5) or progress in {1, 100}:
                self.logged_progress[job_id] = progress
                should_log = True

        if should_log:
            logger.info("filter job progress job_id=%s progress=%s", job_id, progress)

    def _run_job(self, job_id: str, file_id: str, output_path, config: FilterConfig) -> None:
        try:
            logger.info("starting filter job job_id=%s source_file_id=%s", job_id, file_id)
            self._update_progress(job_id, 1)
            source_metadata = self.store.get_metadata(file_id)
            result = process_filtered_view(
                source_path=self.store._file_path(file_id),
                output_path=output_path,
                config=config,
                progress_callback=lambda progress: self._update_progress(job_id, progress),
            )

            view_metadata = ViewMetadata(
                viewId=job_id,
                filename=f"{source_metadata.filename} [filtered]",
                sizeBytes=result.size_bytes,
                logicalBitLength=result.logical_bit_length,
                createdAt=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                sourceFileId=file_id,
                groupBitLengths=result.group_bit_lengths,
                isFiltered=True,
            )
            self.store.save_view_metadata(view_metadata)
            self._set_job_status(job_id, status="completed", progress=100, viewId=job_id)
            logger.info(
                "completed filter job job_id=%s output_size_bytes=%s logical_bit_length=%s groups=%s",
                job_id,
                result.size_bytes,
                result.logical_bit_length,
                0 if result.group_bit_lengths is None else len(result.group_bit_lengths),
            )
        except Exception as exc:
            self.store.delete_view(job_id)
            self._set_job_status(job_id, status="failed", error=str(exc), progress=100)
            logger.exception("filter job failed job_id=%s source_file_id=%s", job_id, file_id)
        finally:
            with self.lock:
                self.logged_progress.pop(job_id, None)
