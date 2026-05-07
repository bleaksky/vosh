import { useEffect, useRef, useState } from 'react';
import { exportProfile, importProfile } from '../lib/session';

interface Props {
  open: boolean;
  onClose: () => void;
  onError: (message: string) => void;
}

export function SettingsDrawer({ open, onClose, onError }: Props) {
  const [toml, setToml] = useState('');
  const [status, setStatus] = useState('');
  const [loading, setLoading] = useState(false);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoading(true);
    exportProfile()
      .then((text) => {
        if (cancelled) return;
        setToml(text);
        setStatus('loaded current profile');
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
      const warnings = await importProfile(toml);
      if (warnings.length === 0) {
        setStatus('profile applied');
      } else {
        setStatus(`applied with ${warnings.length} warning(s): ${warnings.join('; ')}`);
      }
    } catch (e) {
      setStatus(`apply error ${String(e)}`);
      onError(String(e));
    }
  };

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(toml);
      setStatus('copied to clipboard');
    } catch (e) {
      setStatus(`clipboard error ${String(e)}`);
    }
  };

  const handlePaste = async () => {
    try {
      const text = await navigator.clipboard.readText();
      setToml(text);
      setStatus('pasted from clipboard');
    } catch (e) {
      setStatus(`clipboard error ${String(e)}`);
    }
  };

  const handleDownload = () => {
    const blob = new Blob([toml], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'mudclient-profile.toml';
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
      setToml(text);
      setStatus(`loaded file ${file.name}`);
    };
    reader.onerror = () => setStatus('file read failed');
    reader.readAsText(file);
  };

  return (
    <aside className="drawer" role="dialog" aria-label="profile editor">
      <header className="drawer-header">
        <h2>Profile</h2>
        <button type="button" onClick={onClose} aria-label="close">
          ×
        </button>
      </header>
      <textarea
        className="drawer-textarea"
        value={toml}
        spellCheck={false}
        onChange={(e) => setToml(e.target.value)}
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
          <input
            ref={fileInputRef}
            type="file"
            accept="text/plain,.toml"
            onChange={handleUpload}
            hidden
          />
        </label>
      </div>
      <div className="drawer-status">{status}</div>
    </aside>
  );
}
