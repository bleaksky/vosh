import { useEffect, useState } from 'react';
import { exportTriggers, importTriggers } from '../lib/session';

interface Props {
  open: boolean;
  onClose: () => void;
  onError: (message: string) => void;
}

export function TriggersDrawer({ open, onClose, onError }: Props) {
  const [json, setJson] = useState('[]');
  const [status, setStatus] = useState<string>('');
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoading(true);
    exportTriggers()
      .then((text) => {
        if (cancelled) return;
        setJson(text);
        setStatus('loaded');
      })
      .catch((e) => {
        if (cancelled) return;
        setStatus(`load error ${String(e)}`);
        onError(String(e));
      })
      .finally(() => {
        if (cancelled) return;
        setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, onError]);

  if (!open) return null;

  const handleApply = async () => {
    try {
      const count = await importTriggers(json);
      setStatus(`applied ${count} trigger(s)`);
    } catch (e) {
      setStatus(`apply error ${String(e)}`);
      onError(String(e));
    }
  };

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(json);
      setStatus('copied to clipboard');
    } catch (e) {
      setStatus(`clipboard error ${String(e)}`);
    }
  };

  const handlePaste = async () => {
    try {
      const text = await navigator.clipboard.readText();
      setJson(text);
      setStatus('pasted from clipboard');
    } catch (e) {
      setStatus(`clipboard error ${String(e)}`);
    }
  };

  const handleDownload = () => {
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'mudclient-triggers.json';
    a.click();
    URL.revokeObjectURL(url);
    setStatus('download started');
  };

  const handleUpload = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const text = String(reader.result ?? '');
      setJson(text);
      setStatus(`loaded file ${file.name}`);
    };
    reader.onerror = () => setStatus('file read failed');
    reader.readAsText(file);
  };

  return (
    <aside className="drawer" role="dialog" aria-label="trigger editor">
      <header className="drawer-header">
        <h2>Triggers</h2>
        <button type="button" onClick={onClose} aria-label="close triggers">
          ×
        </button>
      </header>
      <textarea
        className="drawer-textarea"
        value={json}
        spellCheck={false}
        onChange={(e) => setJson(e.target.value)}
        disabled={loading}
        rows={20}
      />
      <div className="drawer-actions">
        <button type="button" onClick={handleApply}>
          apply
        </button>
        <button type="button" onClick={handleCopy}>
          copy
        </button>
        <button type="button" onClick={handlePaste}>
          paste
        </button>
        <button type="button" onClick={handleDownload}>
          download
        </button>
        <label className="upload-label">
          upload
          <input type="file" accept="application/json,.json" onChange={handleUpload} hidden />
        </label>
      </div>
      <div className="drawer-status">{status}</div>
    </aside>
  );
}
