import { afterEach, expect, mock, test } from 'bun:test';
const calls: Array<[string,any]> = [];
let explicit: string|null = null;
mock.module('@tauri-apps/api/core', () => ({invoke: async (command:string,args:any) => {
  calls.push([command,args]);
  if (command==='api_get_meeting_summary_language') return {language:explicit,storage:'metadata'};
  if (command==='api_get_meeting_detected_summary_language') return {language:'en',storage:'metadata'};
  if (command==='api_detect_transcript_summary_language') return {language:args.transcriptTexts.join('').includes('Привет')?'ru':'en',reason:'detected'};
  throw new Error(`Unexpected provider or metadata write: ${command}`);
}}));
mock.module('@/components/Sidebar/SidebarProvider',()=>({useSidebar:()=>({})}));
mock.module('sonner',()=>({toast:{warning:()=>{}}}));
const {resolveSummaryLanguage} = await import(process.env.ISSUE14_LANGUAGE_MODULE ?? '../../src/hooks/meeting-details/useSummaryGeneration');
afterEach(()=>{calls.length=0;explicit=null;});
test('R1 stale detected language is not reused after selection or text correction',async()=>{
 expect(await resolveSummaryLanguage('m',['English live'])).toBe('en');
 expect(await resolveSummaryLanguage('m',['Привет selected candidate'])).toBe('ru');
 expect(await resolveSummaryLanguage('m',['English corrected text'])).toBe('en');
 expect(calls.filter(([name])=>name==='api_detect_transcript_summary_language')).toHaveLength(3);
 expect(calls.some(([name])=>name==='api_get_meeting_detected_summary_language')).toBe(false);
 expect(calls.some(([name])=>name.startsWith('api_save_')||name==='api_generate_summary')).toBe(false);
});
test('R1 explicit language wins without detection, provider calls or provenance writes',async()=>{
 explicit='de';
 expect(await resolveSummaryLanguage('m',['Привет'])).toBe('de');
 expect(calls.map(([name])=>name)).toEqual(['api_get_meeting_summary_language']);
});
