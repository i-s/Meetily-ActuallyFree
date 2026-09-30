'use client';
import { useEffect, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { CodexCliStatus, codexCliBlockingReason, getCodexCliStatus, testCodexCliConnection } from '@/lib/codex-cli';

export function CodexCliSettings({path, model, onPathChange, onStatusChange}: {
  path: string; model: string; onPathChange: (path: string) => void;
  onStatusChange: (status: CodexCliStatus | null) => void;
}) {
  const [status, setStatus] = useState<CodexCliStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const request = useRef(0);
  const refresh = async () => {
    const id = ++request.current;
    setChecking(true);
    setResult(null);
    try {
      const next = await getCodexCliStatus(path.trim());
      if (request.current === id) { setStatus(next); onStatusChange(next); }
    } catch (error) {
      if (request.current === id) {
        const next = {installed: false, logged_in: false, error: String(error)};
        setStatus(next); onStatusChange(next);
      }
    } finally { if (request.current === id) setChecking(false); }
  };
  // Editing or asynchronously loading a saved path invalidates readiness. Debounce
  // probes so typing does not spawn a process per character; ignore stale replies.
  useEffect(() => {
    request.current++;
    setStatus(null); onStatusChange(null); setResult(null); setChecking(true);
    const timer = setTimeout(() => void refresh(), 400);
    return () => { clearTimeout(timer); request.current++; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path]);
  const reason = codexCliBlockingReason(status);
  return <div className="space-y-4 border-t pt-4">
    <div><h4 className="text-sm font-semibold">Codex CLI</h4>
      <p className="text-xs text-muted-foreground mt-1">Summaries and Ask AI use Codex installed on this computer with your ChatGPT subscription. Sign in using <code>codex login</code> in a terminal. No API key is needed. Requests use your subscription limits.</p>
    </div>
    <div className="rounded-xl border p-3 text-sm" role="status">
      {checking ? 'Checking Codex CLI…' : reason || `Ready · ${status?.version || 'Codex CLI'} · signed in with ChatGPT`}
      {status?.path && <p className="break-all text-xs text-muted-foreground">{status.path}</p>}
    </div>
    <div><Label htmlFor="codex-cli-path">CLI path (optional)</Label>
      <Input id="codex-cli-path" value={path} onChange={event => onPathChange(event.target.value)} placeholder="Leave empty to find codex automatically" />
      <p className="text-xs text-muted-foreground mt-1">Choose the executable if a Dock launch cannot find your installation. The default model is selected by Codex; personal tools and project instructions are disabled.</p>
    </div>
    <div className="flex gap-2">
      <Button type="button" variant="outline" onClick={() => void refresh()} disabled={checking}>Re-check</Button>
      <Button type="button" variant="outline" disabled={testing || checking || !!reason} onClick={async () => {
        const id = request.current; setTesting(true); setResult(null);
        try { const response = await testCodexCliConnection(path.trim(), model); if (request.current === id) setResult(response.message); }
        catch (error) { if (request.current === id) setResult(String(error)); }
        finally { setTesting(false); }
      }}>{testing ? 'Testing…' : 'Test AI call'}</Button>
    </div>
    {result && <p className="text-xs" role="status">{result}</p>}
  </div>;
}
