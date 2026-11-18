import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
let cleanup: (() => void) | undefined;
let exists = true;
mock.module('@tauri-apps/api/event',()=>({listen:async(_name:string,fn:any)=>{cleanup=fn;return()=>{cleanup=undefined;};}}));
mock.module('@tauri-apps/api/core',()=>({convertFileSrc:(path:string)=>path}));
mock.module('@/lib/workspace-api',()=>({getMeetingAudio:async()=>({path:exists?'/generated/audio.mp4':null}),onWorkspaceChange:()=>()=>{}}));
const elements:any[]=[];
class FakeAudio {
  src=''; preload=''; duration=42; paused=true;
  onloadedmetadata?:()=>void; onplay?:()=>void; onpause?:()=>void;
  pause(){this.paused=true;this.onpause?.();}
  removeAttribute(){this.src='';} load(){} play(){this.paused=false;this.onplay?.();return Promise.resolve();}
  constructor(){elements.push(this);}
}
Object.assign(globalThis,{Audio:FakeAudio,window:{addEventListener(){},removeEventListener(){}},requestAnimationFrame:()=>1,cancelAnimationFrame(){}});
const {useMeetingAudio}=await import('../../src/hooks/useMeetingAudio');
let controls:any;
function Harness(){controls=useMeetingAudio('generated');return <p>{controls.status}</p>;}
test('native cleanup notification closes an existing player and reports removed audio unavailable',async()=>{
 let root:any; await act(async()=>{root=create(<Harness/>);});
 await act(async()=>elements.at(-1).onloadedmetadata()); expect(controls.status).toBe('ready');
 exists=false; await act(async()=>cleanup?.());
 expect(controls.status).toBe('unavailable');expect(controls.path).toBe(null);expect(elements[0].src).toBe('');
 await act(async()=>root.unmount()); expect(cleanup).toBeUndefined();
});
