import React from 'react';
import {act,create} from 'react-test-renderer';
import {expect,mock,test} from 'bun:test';
const pushed:string[]=[];
const empty=()=>null;
const wrap=({children}:any)=><div>{children}</div>;
mock.module('next/navigation',()=>({useRouter:()=>({push:(url:string)=>pushed.push(url)})}));
mock.module('lucide-react',()=>Object.fromEntries(['CalendarDays','Check','Play','Plus','Trash2','UserRound'].map(name=>[name,empty])));
mock.module('sonner',()=>({toast:{error(){}}}));
mock.module('@/contexts/WorkspaceContext',()=>({useWorkspace:()=>({people:[]})}));
mock.module('@/hooks/useUserName',()=>({useUserName:()=>''}));
mock.module('@/lib/workspace-api',()=>({announceChange(){},createActionItem:async()=>{},deleteActionItem:async()=>{},updateActionItem:async()=>{}}));
mock.module('@/components/ui/checkbox',()=>({Checkbox:empty}));
mock.module('@/components/ui/avatar',()=>({Avatar:empty}));
mock.module('@/components/ui/combobox',()=>({Combobox:empty}));
mock.module('@/components/ui/popover',()=>({Popover:wrap,PopoverContent:wrap,PopoverTrigger:wrap}));
mock.module('@/components/ui/input',()=>({Input:empty}));
mock.module('@/components/ui/tooltip',()=>({Hint:wrap}));
mock.module('@/components/meeting/AudioPlayerBar',()=>({formatPlayback:(time:number)=>String(time)}));
const {ActionItemsList}=await import(process.env.ISSUE14_TASK_MODULE??'../../src/components/actions/ActionItemsList');
test('R1 task source links retain fractional source time and support audio/meeting fallback',async()=>{
 for(const [transcriptId,audioTime] of [['selected-row',1.25],[null,9.25],[null,null]] as const){
  let root:any;
  const item:any={id:'task',meetingId:'m',meetingTitle:'Source meeting',meetingCreatedAt:'2026-11-18',text:'Keep task',done:true,edited:true,source:'user',transcriptId,audioTime};
  await act(async()=>{root=create(<ActionItemsList items={[item]} onItemsChange={()=>{}} allowAdd={false} showOwner={false}/>);});
  const source=root.root.findAllByType('button').find((node:any)=>node.findAllByType('span').some((span:any)=>span.props.children==='Source meeting'));
  await act(async()=>source.props.onClick());
  const url=new URL(pushed.at(-1)!,'https://local.invalid');
  expect(url.searchParams.get('id')).toBe('m');
  expect(url.searchParams.get('t')).toBe(transcriptId);
  expect(url.searchParams.get('ts')).toBe(audioTime===null?null:String(audioTime));
  await act(async()=>root.unmount());
 }
});
