import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
let confirmed = 0, prepared: any[] = [], fail = false;
const row = { meetingId: 'old', title: 'Generated old meeting', createdAt: '2020-01-01', files: [{name:'audio.mp4',bytes:24},{name:'mic.mp4',bytes:24}], bytes:48, blocked:null };
const inventory = { totalBytes:96, usageComplete:true, recordings:[row,{...row,meetingId:'busy',title:'Busy meeting',blocked:'Background job retains this audio'}] };
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command:string,args:any) => {
  if(command==='api_audio_storage') return inventory;
  if(command==='api_prepare_audio_cleanup') { prepared.push(args); return {token:'scoped',recordings:[row],bytes:48}; }
  if(command==='api_confirm_audio_cleanup') { confirmed++; if(fail) throw new Error('database unavailable; review again'); return {removedBytes:24,files:[{meetingId:'old',name:'audio.mp4',status:'removed',removedBytes:24,error:null},{meetingId:'old',name:'mic.mp4',status:'error',removedBytes:0,error:'Permission denied'}]}; }
  throw new Error(command);
} }));
mock.module('@tauri-apps/api/event', () => ({ listen:async()=>()=>{} }));
mock.module('@/lib/workspace-api',()=>({announceChange(){},onWorkspaceChange:()=>()=>{}}));
const Wrap=({children}:any)=><div>{children}</div>;
mock.module('@/components/ui/dialog',()=>({ Dialog:({open,children}:any)=>open?<div>{children}</div>:null, DialogContent:Wrap,DialogHeader:Wrap,DialogTitle:Wrap,DialogDescription:Wrap,DialogFooter:Wrap }));
mock.module('@/components/ui/button',()=>({Button:({children,...props}:any)=><button {...props}>{children}</button>}));
Object.assign(globalThis,{window:{addEventListener(){},removeEventListener(){}}});
const { AudioStorage }=await import('../../src/components/meetings/AudioStorage');
const button=(root:ReactTestRenderer,text:string)=>root.root.findAllByType('button').find(b=>b.children.join('')===text)!;
const click=async(root:ReactTestRenderer,text:string)=>act(async()=>{await button(root,text).props.onClick();});
test('normal storage flow requires unchecked explicit scope and lists retained materials before consent',async()=>{
  let root!:ReactTestRenderer; await act(async()=>{root=create(<AudioStorage/>);});
  expect(JSON.stringify(root.toJSON())).toContain('96 B');
  await click(root,'Review audio cleanup');
  expect(JSON.stringify(root.toJSON())).toContain('30 days');
  expect(JSON.stringify(root.toJSON())).toContain('not a storage limit');
  expect(root.root.findByProps({'aria-label':'Select audio for Generated old meeting'}).props.checked).toBe(false);
  expect(root.root.findByProps({'aria-label':'Select audio for Busy meeting'}).props.disabled).toBe(true);
  expect(button(root,'Review selected audio').props.disabled).toBe(true); expect(confirmed).toBe(0);
  await act(async()=>root.root.findByProps({'aria-label':'Select audio for Generated old meeting'}).props.onChange({target:{checked:true}}));
  await click(root,'Review selected audio');
  expect(prepared.at(-1)).toEqual({meetingIds:['old']}); expect(confirmed).toBe(0);
  const text=JSON.stringify(root.toJSON()); for(const value of ['audio.mp4','mic.mp4','manual corrections','Notes','summaries','queue','Playback','ASR']) expect(text).toContain(value);
  await click(root,'Delete listed audio'); expect(confirmed).toBe(1);
  const result=JSON.stringify(root.toJSON()); expect(result).toContain('24 B');expect(result).toContain('Permission denied'); expect(result).toContain('not a measurement of freed disk space');
  expect(button(root,'Review selected audio').props.disabled).toBe(true);
  await act(async()=>root.unmount());
});
test('lost confirmation response never claims deletion success and requires fresh scope',async()=>{
  fail=true; let root!:ReactTestRenderer; await act(async()=>{root=create(<AudioStorage/>);});
  await click(root,'Review audio cleanup'); await act(async()=>root.root.findByProps({'aria-label':'Select audio for Generated old meeting'}).props.onChange({target:{checked:true}}));
  await click(root,'Review selected audio'); await click(root,'Delete listed audio');
  expect(JSON.stringify(root.toJSON())).toContain('database unavailable'); expect(JSON.stringify(root.toJSON())).not.toContain('Removed file bytes');
  expect(root.root.findAllByType('button').some(b=>b.children.join('')==='Delete listed audio')).toBe(false);
  await act(async()=>root.unmount()); fail=false;
});
