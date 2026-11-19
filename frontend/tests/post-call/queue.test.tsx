import React from 'react';
import {act,create,type ReactTestRenderer} from 'react-test-renderer';
import {expect,mock,test} from 'bun:test';
const calls:Array<[string,any]>=[];
let jobs:any[]=[{meeting_id:'a',status:'waiting',stage:'summary',epoch:2,candidate_id:'post-call-refined:a',error:'Offline/quota; retry later',result_json:JSON.stringify({summary:[],decisions:[],tasks:[],open_questions:[],uncertainty:['No confirmed owners']}),result_version:'live-a',result_transcript_revision:3,result_notes_revision:7,result_vocabulary_revision:7,vocabulary_revision:7,result_current:false}];
let fail=false;
mock.module('@tauri-apps/api/core',()=>({invoke:async(command:string,args:any)=>{
 calls.push([command,args]);
 if(command==='post_call_list'){if(fail)throw new Error('Queue temporarily unavailable');return structuredClone(jobs);}
 if(command==='post_call_control'){jobs[0].status=args.action==='defer'?'deferred':args.action==='cancel'?'cancelled':'queued';return;}
 if(command==='post_call_results')return [{version_id:'live-a',transcript_revision:3,notes_revision:7,vocabulary_revision:jobs[0].result_vocabulary_revision,result_json:jobs[0].result_json,created_at:'2026-11-18'}];
 throw new Error(`Forbidden fallback: ${command}`);
}}));
const listeners=new Map<string,Set<(event:any)=>void>>();
mock.module('@tauri-apps/api/event',()=>({listen:async(name:string,handler:(event:any)=>void)=>{const set=listeners.get(name)??new Set();set.add(handler);listeners.set(name,set);return()=>set.delete(handler);}}));
const {PostCallQueue,PostCallQueueNotice}=await import('../../src/components/PostCallQueue');
const button=(root:ReactTestRenderer,text:string)=>root.root.findAllByType('button').find(b=>b.children.includes(text))!;
test('visible persisted stages, retry/defer, historical result and no model/Notes/selected writes from WebView',async()=>{
 let root!:ReactTestRenderer;await act(async()=>{root=create(<PostCallQueue/>);});
 expect(JSON.stringify(root.toJSON())).toContain('Offline/quota');expect(JSON.stringify(root.toJSON())).toContain('Outdated');
 expect(JSON.stringify(root.toJSON())).toContain('Notes are personal');expect(JSON.stringify(root.toJSON())).toContain('vocabulary:global:7');
 await act(async()=>{button(root,'Retry').props.onClick();});expect(calls).toContainEqual(['post_call_control',{meetingId:'a',action:'retry'}]);
 await act(async()=>{button(root,'Defer').props.onClick();});expect(JSON.stringify(root.toJSON())).toContain('Deferred');
 await act(async()=>{button(root,'History').props.onClick();});expect(calls).toContainEqual(['post_call_results',{meetingId:'a'}]);expect(JSON.stringify(root.toJSON())).toContain('No confirmed owners');
 expect(calls.every(([name])=>['post_call_list','post_call_control','post_call_results'].includes(name))).toBe(true);
 await act(async()=>root.unmount());
});
test('observing a queue failure never starts a competing job or discards previous native results',async()=>{
 fail=true;let root!:ReactTestRenderer;await act(async()=>{root=create(<PostCallQueue/>);});expect(JSON.stringify(root.toJSON())).toContain('Queue temporarily unavailable');
 expect(calls.some(([name])=>['start_retranscription_command','api_generate_summary','select_transcript_version','api_write_notes_document'].includes(name))).toBe(false);
 await act(async()=>root.unmount());fail=false;
});

test('native local progress is meeting scoped and page handoff never starts a second job',async()=>{
 jobs[0]={...jobs[0],status:'running',stage:'local',epoch:9};
 let root!:ReactTestRenderer;await act(async()=>{root=create(<PostCallQueue/>);});
 await act(async()=>{for(const callback of listeners.get('retranscription-progress')??[])callback({payload:{meeting_id:'other',message:'wrong meeting'}});});
 expect(JSON.stringify(root.toJSON())).not.toContain('wrong meeting');
 await act(async()=>{for(const callback of listeners.get('retranscription-progress')??[])callback({payload:{meeting_id:'a',message:'Refining speakers locally'}});});
 expect(JSON.stringify(root.toJSON())).toContain('Refining speakers locally');await act(async()=>root.unmount());
 const before=calls.length;let consumed=0;
 await act(async()=>{root=create(<PostCallQueueNotice enabled meetingId="a" onComplete={()=>{consumed++;}}/>);});
 expect(consumed).toBe(1);expect(calls.length).toBe(before);await act(async()=>root.unmount());
});

test('cached history never inherits freshness from a newer published tuple',async()=>{
 const result=(id:string)=>JSON.stringify({summary:[{text:'Speech',evidence:[{source:'transcript',row_id:id,quote:'Exact speech'}],owner:null,deadline:null}],decisions:[],tasks:[],open_questions:[],uncertainty:[]});
 jobs[0]={...jobs[0],status:'complete',stage:'summary',result_current:true,result_version:'live-a',result_transcript_revision:3,result_notes_revision:7,result_json:result('old-row')};
 let root!:ReactTestRenderer;await act(async()=>{root=create(<PostCallQueue/>);});await act(async()=>{button(root,'History').props.onClick();});
 jobs[0]={...jobs[0],result_version:'selected-new',result_transcript_revision:10,result_notes_revision:8,result_json:result('new-row')};
 await act(async()=>{for(const callback of listeners.get('post-call-queue-changed')??[])callback({payload:null});});
 const links=root.root.findAllByType('a').map(a=>String(a.props.href));expect(links.some(href=>href.includes('t=old-row'))).toBe(false);expect(links.some(href=>href.includes('t=new-row'))).toBe(true);
 await act(async()=>root.unmount());
});
