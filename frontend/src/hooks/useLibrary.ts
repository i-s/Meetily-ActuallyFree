import { useEffect, useRef, useState } from 'react';
import { libraryMaterials, searchLibrary, type LibraryMaterial } from '@/lib/library-api';
import { onWorkspaceChange } from '@/lib/workspace-api';
import type { GlobalSearchResult } from '@/types';

/** A result belongs to the exact query generation, not just the latest request
 * to finish. No live transcript/chat state is read or written here. */
export function useLibrarySearch(query: string) {
  const [state, setState] = useState<{ query: string; hits: GlobalSearchResult[]; loading: boolean; error: string | null }>({ query: '', hits: [], loading: false, error: null });
  const generation = useRef(0);
  const [revision, setRevision] = useState(0);
  useEffect(() => onWorkspaceChange(['notes', 'meetings'], () => setRevision(value => value + 1)), []);
  useEffect(() => {
    const run = ++generation.current;
    const needle = query.trim();
    if (!needle) { setState({ query: needle, hits: [], loading: false, error: null }); return; }
    setState({ query: needle, hits: [], loading: true, error: null });
    const timer = setTimeout(async () => {
      try {
        const hits = await searchLibrary(needle);
        if (generation.current === run) setState({ query: needle, hits, loading: false, error: null });
      } catch (error) {
        if (generation.current === run) setState({ query: needle, hits: [], loading: false, error: error instanceof Error ? error.message : String(error) });
      }
    }, 220);
    return () => { clearTimeout(timer); ++generation.current; };
  }, [query, revision]);
  const current = state.query === query.trim() ? state : { query: query.trim(), hits: [], loading: !!query.trim(), error: null };
  return { ...current, retry: () => setRevision(value => value + 1) };
}

export function useLibraryMaterials() {
  const [materials, setMaterials] = useState<Map<string, LibraryMaterial>>(new Map());
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    let generation = 0;
    let active = true;
    const load = async () => {
      const run = ++generation;
      try {
        const rows = await libraryMaterials();
        if (active && run === generation) { setMaterials(new Map(rows.map(row => [row.id, row]))); setError(null); }
      } catch (error) {
        if (active && run === generation) setError(error instanceof Error ? error.message : String(error));
      }
    };
    void load();
    const changed = () => void load();
    const stop = onWorkspaceChange(['notes', 'meetings'], changed);
    window.addEventListener('focus', changed);
    return () => { active = false; ++generation; stop(); window.removeEventListener('focus', changed); };
  }, [revision]);
  return { materials, error, retry: () => setRevision(value => value + 1) };
}
