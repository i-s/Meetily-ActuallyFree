import {afterEach,expect,mock,test} from 'bun:test';
const full={markdown:'## Overview\nCurrent summary\n\n## Action items\n- Prepare detailed delivery report'};
const calls:Array<[string,any]>=[];let stale=false,events=0,ids:string[]=[],summary:any=full;
mock.module('@tauri-apps/api/core',()=>({invoke:async(name:string,args:any)=>{
 calls.push([name,args]);
 if(name==='api_get_summary')return {data:summary};
 if(name==='api_action_sync_context')return stale?null:{meetingId:args.meetingId,versionId:'selected',revision:4,summaryToken:'owned',summary,transcripts:[{id:'native-current',text:'Prepare detailed delivery report',audio_start_time:8}]};
 if(name==='api_commit_action_sync')return [];
 throw new Error(`Unexpected unguarded call: ${name}`);
}}));
mock.module('@/lib/workspace-api',()=>({announceChange:()=>{events++;},listUnsyncedActionMeetings:async()=>ids}));
(globalThis as any).sessionStorage={getItem:()=>null,setItem(){}};
const {syncMeetingActionItems,backfillActionItems}=await import('../../src/lib/action-item-sync');
afterEach(()=>{calls.length=0;stale=false;events=0;ids=[];summary=full;});

test('real extraction uses checked native summary and anchors, never stale supplied transcript',async()=>{
 const outcome=await syncMeetingActionItems('m',full,[{id:'stale-ui',text:'Prepare detailed delivery report',timestamp:99}]);
 expect(outcome.changed).toBe(true);
 const commit=calls.find(([name])=>name==='api_commit_action_sync')![1];
 expect(commit.context.summaryToken).toBe('owned');expect(commit.context.revision).toBe(4);
 expect(commit.items[0].transcriptId).toBe('native-current');expect(commit.items[0].audioTime).toBe(8);
 expect(commit.summary.markdown).toBe('## Overview\nCurrent summary');
 expect(calls.map(([name])=>name)).toEqual(['api_action_sync_context','api_commit_action_sync']);
 expect(events).toBe(1);
});
test('stale context is a no-op without task reconciliation, trim or event',async()=>{
 stale=true;expect(await syncMeetingActionItems('m',full)).toEqual({changed:false});
 expect(calls.map(([name])=>name)).toEqual(['api_action_sync_context']);expect(events).toBe(0);
});
test('no-section backfill uses ownership-checked marker-only commit',async()=>{
 ids=['m'];summary={markdown:'Summary without tasks'};await backfillActionItems();
 const commit=calls.find(([name])=>name==='api_commit_action_sync')![1];
 expect(commit.items).toEqual([]);expect(commit.summary).toBeNull();expect(commit.context.summaryToken).toBe('owned');
 expect(calls.map(([name])=>name)).toEqual(['api_get_summary','api_action_sync_context','api_commit_action_sync']);
});
test('stale no-section backfill cannot fall back to meeting-only sync',async()=>{
 ids=['m'];summary={markdown:'Summary without tasks'};stale=true;await backfillActionItems();
 expect(calls.map(([name])=>name)).toEqual(['api_get_summary','api_action_sync_context']);expect(events).toBe(0);
});

test('stale caller cannot stamp its intent onto a newly captured different summary',async()=>{
 summary={markdown:'## Overview\nNew job\n\n## Action items\n- Prepare another delivery report'};
 expect(await syncMeetingActionItems('m',full)).toEqual({changed:false});
 expect(calls.map(([name])=>name)).toEqual(['api_action_sync_context']);expect(events).toBe(0);
});

test('normal UI markdown projection accepts canonical metadata while deriving from canonical summary',async()=>{
 summary={...full,original_markdown:'Original native summary',language:'en'};
 const outcome=await syncMeetingActionItems('m',full);
 expect(outcome.changed).toBe(true);expect(outcome.summary?.original_markdown).toBe('Original native summary');
 expect(calls.find(([name])=>name==='api_commit_action_sync')![1].context.summary).toEqual(summary);
});
