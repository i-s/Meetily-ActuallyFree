import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { afterEach, expect, mock, test } from 'bun:test';
let meeting='m', version='v1', revision=1, global='', failScan=false, failConfirm=false;
let text='Обсудили OpenAPI, СУБД, M4 и ЁлкаAPI.', calls:{name:string,args:any}[]=[];
let holdScan=false, holdConfirm=false, releaseScan=()=>{},releaseConfirm=()=>{};
const view=()=>({version_id:version,revision,segments:[{id:'row',text,audio_start_time:12}]});
mock.module('@tauri-apps/api/event',()=>({listen:async()=>()=>{}}));
mock.module('@tauri-apps/api/core',()=>({invoke:async(name:string,args:any)=>{
 calls.push({name,args});
 if(name==='list_transcript_versions') return {selected_id:version,revision,versions:[],audio_available:true};
 if(name==='api_get_vocabulary_suggestion_context') {
  const captured=view(); if(holdScan) await new Promise<void>(r=>releaseScan=r);
  if(failScan) throw new Error('context unavailable'); return captured;
 }
 if(name==='api_get_whisper_vocabulary') return {global,meeting:'',approvedGlobal:global,vocabularyRevision:1};
 if(name==='api_confirm_vocabulary_suggestion') {
  if(holdConfirm) await new Promise<void>(r=>releaseConfirm=r);
  if(failConfirm) throw new Error('confirmation unavailable');
  if(args.meetingId!==meeting||args.versionId!==version||args.revision!==revision) throw new Error('Suggestion source changed');
  global=global?`${global}\n${args.term}`:args.term; return global;
 }
 if(name==='api_confirm_vocabulary_term') {global=global?`${global}\n${args.term}`:args.term;return global;}
 throw new Error(`Unexpected command ${name}`);
}}));
mock.module('@/components/ui/button',()=>({Button:({children,...props}:any)=><button {...props}>{children}</button>}));
const {TranscriptVersions}=await import('../../src/components/MeetingDetails/TranscriptVersions');
let root:ReactTestRenderer; let barrier=0;
const props=(id=meeting)=>({meetingId:id,onChanged:()=>{},onBeforeChange:async()=>{barrier++;return ()=>{};}});
const buttons=(label:string)=>root.root.findAllByType('button').filter(n=>n.props.children===label);
const click=async(label:string,index=0)=>act(async()=>{buttons(label)[index].props.onClick();});
const rendered=()=>JSON.stringify(root.toJSON());
const writes=()=>calls.filter(c=>!['list_transcript_versions','api_get_vocabulary_suggestion_context','api_get_whisper_vocabulary'].includes(c.name));
async function mount(){await act(async()=>{root=create(<TranscriptVersions {...props()}/>);});}
afterEach(async()=>{await act(async()=>root?.unmount()); meeting='m';version='v1';revision=1;global='';text='Обсудили OpenAPI, СУБД, M4 и ЁлкаAPI.';calls=[];failScan=failConfirm=holdScan=holdConfirm=false;barrier=0;});
test('ordinary version-browser scan shows exact source and decline/close/rescan write nothing',async()=>{
 await mount(); expect(calls.map(c=>c.name)).toEqual(['list_transcript_versions']);
 await click('Предложить термины'); expect(rendered()).toContain('OpenAPI');expect(rendered()).toContain('Обсудили');
 await click('Отклонить'); expect(rendered()).not.toContain('"children":["OpenAPI"]');
 await click('Закрыть предложения'); await click('Предложить термины');
 expect(buttons('Запомнить написание')).toHaveLength(0);expect(writes()).toEqual([]);expect(global).toBe('');expect(barrier).toBe(0);
});
test('only explicit acceptance saves exact selected source spelling; duplicate clicks are bounded',async()=>{
 await mount();await click('Предложить термины');holdConfirm=true;
 await act(async()=>{buttons('Запомнить написание')[0].props.onClick();buttons('Запомнить написание')[0].props.onClick();});
 expect(writes()).toHaveLength(1);expect(writes()[0]).toEqual({name:'api_confirm_vocabulary_suggestion',args:{meetingId:'m',versionId:'v1',revision:1,transcriptId:'row',term:'OpenAPI'}});
 await act(async()=>releaseConfirm());expect(global).toBe('OpenAPI');expect(rendered()).toContain('сохранено');expect(barrier).toBe(0);
 await click('Предложить термины');expect(buttons('Запомнить написание')).toHaveLength(3);
});
test('existing case variants are not offered and error or empty scan leaves manual confirmation usable',async()=>{
 global='openapi\nСУБД\nM4\nЁлкаAPI';await mount();await click('Предложить термины');expect(buttons('Запомнить написание')).toHaveLength(0);
 failScan=true;await click('Предложить термины');expect(rendered()).toContain('context unavailable');expect(writes()).toEqual([]);
 await click('Добавить вручную');const input=root.root.findByType('input');await act(async()=>input.props.onChange({target:{value:'Елена'}}));await click('Запомнить');expect(global).toBe('openapi\nСУБД\nM4\nЁлкаAPI\nЕлена');
});
test('confirmation failure leaves dictionary unchanged and permits explicit retry',async()=>{
 await mount();await click('Предложить термины');failConfirm=true;await click('Запомнить написание');expect(global).toBe('');expect(rendered()).toContain('confirmation unavailable');
 failConfirm=false;await click('Запомнить написание');expect(global).toBe('OpenAPI');
});
test('held old scan cannot publish after revision, version or meeting switch',async()=>{
 await mount();holdScan=true;await click('Предложить термины');revision=2;version='v2';await click('Refresh versions');await act(async()=>releaseScan());
 expect(buttons('Запомнить написание')).toHaveLength(0);expect(writes()).toEqual([]);
 holdScan=false;await click('Предложить термины');expect(buttons('Запомнить написание')).toHaveLength(4);
 holdScan=true;await click('Предложить термины');meeting='other';await act(async()=>root.update(<TranscriptVersions {...props()}/>));await act(async()=>releaseScan());
 expect(buttons('Запомнить написание')).toHaveLength(0);expect(writes()).toEqual([]);
});
test('external source change rejects acceptance; held old completion does not affect new meeting',async()=>{
 await mount();await click('Предложить термины');revision=2;await click('Запомнить написание');expect(global).toBe('');expect(rendered()).toContain('Suggestion source changed');
 revision=1;holdConfirm=true;await click('Запомнить написание');meeting='other';await act(async()=>root.update(<TranscriptVersions {...props()}/>));await act(async()=>releaseConfirm());
 expect(rendered()).not.toContain('сохранено');expect(buttons('Запомнить написание')).toHaveLength(0);
});
test.each(['version','revision'])('a %s-only projection change clears visible and held candidates',async(kind)=>{
 await mount();await click('Предложить термины');expect(buttons('Запомнить написание')).toHaveLength(4);
 if(kind==='version') version='v2';else revision=2;
 await click('Refresh versions');expect(buttons('Запомнить написание')).toHaveLength(0);expect(writes()).toEqual([]);
 holdScan=true;await click('Предложить термины');
 if(kind==='version') version='v3';else revision=3;
 await click('Refresh versions');await act(async()=>releaseScan());
 expect(buttons('Запомнить написание')).toHaveLength(0);expect(writes()).toEqual([]);
});
test('ordinary meeting controls offer and confirm exact decomposed spellings, never fragments',async()=>{
 text='ёлкаAPI ËlkaAPI OpenAPÍClient ControlAPI';
 await mount();await click('Предложить термины');
 expect(root.root.findAllByType('strong').map(n=>n.props.children)).toEqual(['ёлкаAPI','ËlkaAPI','OpenAPÍClient','ControlAPI']);
 expect(writes()).toEqual([]);
 await click('Запомнить написание');
 expect(writes()).toEqual([{name:'api_confirm_vocabulary_suggestion',args:{meetingId:'m',versionId:'v1',revision:1,transcriptId:'row',term:'ёлкаAPI'}}]);
 expect(global).toBe('ёлкаAPI');
});
