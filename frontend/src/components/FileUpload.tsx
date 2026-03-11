import { useState } from 'react';

interface FileUploadProps {
  onUpload: (file: File) => Promise<void>;
  uploading: boolean;
}

export function FileUpload({ onUpload, uploading }: FileUploadProps) {
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selectedFile) {
      setError('Choose a file first.');
      return;
    }

    setError(null);
    try {
      await onUpload(selectedFile);
    } catch (uploadError) {
      setError(uploadError instanceof Error ? uploadError.message : 'Upload failed');
    }
  }

  return (
    <form className="upload-panel" onSubmit={handleSubmit}>
      <div>
        <h1>Bit Viewer</h1>
        <p>Upload a binary file and inspect it as bits, hex, and ASCII without expanding the whole file in the browser.</p>
      </div>
      <label className="file-input">
        <span>Select file</span>
        <input
          type="file"
          onChange={(event) => {
            setSelectedFile(event.target.files?.[0] ?? null);
            setError(null);
          }}
        />
      </label>
      {selectedFile ? <div className="upload-meta">{selectedFile.name}</div> : null}
      {error ? <div className="error-banner">{error}</div> : null}
      <button className="primary-button" type="submit" disabled={uploading || !selectedFile}>
        {uploading ? 'Uploading...' : 'Upload'}
      </button>
    </form>
  );
}
