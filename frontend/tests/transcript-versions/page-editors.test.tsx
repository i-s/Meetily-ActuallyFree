import React, {useEffect, useRef, useState} from 'react';
import {act,create,ReactTestRenderer} from 'react-test-renderer';
import {afterEach,expect,mock,test} from 'bun:test';
const timers = new Map<number,()=>void>(); let timerId=0, reloads=0;
(globalThis as any).window={innerWidth:1200,matchMedia:()=>({matches:false,addEventListener(){},removeEventListener(){}}),addEventListener(){},removeEventListener(){},location:{reload(){reloads++;}},setTimeout:(fn:()=>void)=>{timers.set(++timerId,fn);return timerId;},clearTimeout:(id:number)=>timers.delete(id)};
(globalThis as any).localStorage={getItem:()=>null,setItem(){}};
let savedNotes='', savedSummary='Stored summary', archivedSummary='', mutations=0, failSave=false;
let delayConversion=false;
let version=0; const commands:string[]=[];
const rows=()=>[{id:version?'new-line':'old-line',text:version?'Selected text':'Live text',speaker:'You',audio_start_time:1,audio_end_time:2}];
mock.module('@tauri-apps/api/core',()=>({invoke:async(name:string,args:any)=>{
 commands.push(name);
 if(name==='list_transcript_versions')return {selected_id:version?'new':'live',revision:7,audio_available:true,versions:[{id:'new',kind:'refined',status:'ready',created_at:'2026-11-18',settings_json:'{}'}]};
 if(name==='get_transcript_version')return [{id:'new-line',text:'Selected text',speaker:'You'}];
 if(name==='get_selected_transcript')return {revision:7,segments:rows()};
 if(name==='api_get_meeting_summary_language')return {language:null,storage:'metadata'};
 if(name==='api_save_meeting_summary'){if(failSave)throw new Error('storage unavailable');savedSummary=args.summary.markdown;return;}
 if(name==='select_transcript_version'||name==='correct_transcript_text'){archivedSummary=savedSummary;savedSummary='';version++;mutations++;return;}
 throw new Error(name);
}}));
const empty=()=>null;
mock.module('lucide-react',()=>Object.fromEntries(['AlertTriangle','ChevronDown','FileText','Languages','ListChecks','NotebookPen','RefreshCw','Settings2','Sparkles','Square','Wand2'].map(name=>[name,empty])));
const wrap=({children}:any)=><div>{children}</div>;
mock.module('framer-motion',()=>({motion:{div:wrap}}));
mock.module('sonner',()=>({toast:{success(){},error(){},warning(){}}}));
mock.module('@/lib/meeting-actions',()=>({deleteMeetings:async()=>{},renameMeeting:async()=>true}));
mock.module('@/lib/analytics',()=>({default:{trackPageView(){}}}));
mock.module('@/components/Sidebar/SidebarProvider',()=>({useSidebar:()=>({setCurrentMeeting(){}})}));
mock.module('@/contexts/ConfigContext',()=>({useConfig:()=>({modelConfig:{model:'test'},setModelConfig(){}})}));
mock.module('@/contexts/WorkspaceContext',()=>({useWorkspace:()=>({people:[]})}));
mock.module('@/lib/workspace-api',()=>({getMeetingGroup:async()=>null,setMeetingGroup:async()=>{},announceChange(){},getMeetingNotes:async()=>({markdown:'Stored notes'}),saveMeetingNotes:async(_id:string,markdown:string)=>{if(failSave)throw new Error('storage unavailable');savedNotes=markdown;},listActionItems:async()=>[],onWorkspaceChange:()=>()=>{},askMeeting:async()=>{}}));
mock.module('@/lib/action-item-sync',()=>({syncMeetingActionItems:async()=>({})}));
mock.module('@/hooks/meeting-details/useSummaryGeneration',()=>({useSummaryGeneration:()=>({summaryStatus:'idle',summaryError:null,getSummaryStatusMessage:()=>'',handleGenerateSummary:async()=>{},handleRegenerateSummary:async()=>{},handleStopGeneration(){}})}));
mock.module('@/hooks/meeting-details/useTemplates',()=>({useTemplates:()=>({selectedTemplate:'standard',availableTemplates:[]})}));
mock.module('@/hooks/meeting-details/useCopyOperations',()=>({useCopyOperations:()=>({})}));
mock.module('@/hooks/meeting-details/useMeetingOperations',()=>({useMeetingOperations:()=>({})}));
mock.module('@/hooks/useMeetingAudio',()=>({PLAYBACK_RATES:[],SLOW_PLAYBACK_RATES:[],useMeetingAudio:()=>({status:'unavailable',seek(){}})}));
mock.module('@/hooks/useWaveform',()=>({useWaveform:()=>null}));
mock.module('@/hooks/useLabs',()=>({useLabs:()=>({labs:{},ready:true})}));
mock.module('@/hooks/useUserName',()=>({useUserName:()=>''}));
mock.module('@/components/meeting/MeetingHeader',()=>({MeetingHeader:empty}));
mock.module('@/components/meeting/AudioPlayerBar',()=>({AudioPlayerBar:empty}));
mock.module('@/components/VirtualizedTranscriptView',()=>({VirtualizedTranscriptView:({segments}:any)=><pre>{JSON.stringify(segments)}</pre>}));
mock.module('@/components/MeetingDetails/PostCallProcessingDialog',()=>({PostCallProcessingDialog:empty}));
mock.module('@/components/MeetingDetails/TemplateEditorModal',()=>({TemplateEditorModal:empty}));
mock.module('@/components/MeetingDetails/MeetingExportDialog',()=>({MeetingExportDialog:empty}));
mock.module('@/components/people/PersonCard',()=>({PersonCard:empty}));
mock.module('@/components/people/SpeakerIdentityDialog',()=>({SpeakerIdentityDialog:empty}));
mock.module('@/components/actions/ActionItemsList',()=>({ActionItemsList:empty}));
mock.module('@/components/chat/ChatThread',()=>({ChatThread:empty}));
mock.module('@/components/LanguagePickerPopover',()=>({LanguagePickerPopover:empty}));
mock.module('@/components/ModelSettingsModal',()=>({ModelSettingsModal:empty}));
mock.module('@/components/ui/button',()=>({Button:({children,...p}:any)=><button {...p}>{children}</button>}));
mock.module('@/components/ui/surface',()=>({Skeleton:empty}));
for(const [path,names] of Object.entries({tabs:['Tabs','TabsContent','TabsList','TabsTrigger'],popover:['Popover','PopoverContent','PopoverTrigger'],select:['Select','SelectContent','SelectItem','SelectTrigger','SelectValue'],dialog:['Dialog','DialogContent','DialogDescription','DialogFooter','DialogHeader','DialogTitle']}))mock.module(`@/components/ui/${path}`,()=>Object.fromEntries(names.map(name=>[name,wrap])));
mock.module('@/components/editor/NotesEditor',()=>({NotesEditor:({initialMarkdown,placeholder,onChange,editable,registerFlush}:any)=>{
 const [text,setText]=useState(initialMarkdown??'');
 const pending = useRef<(()=>void)|null>(null);
 useEffect(()=>registerFlush?.(async()=>{pending.current?.();pending.current=null;}),[registerFlush]);
 return <textarea aria-label={placeholder.startsWith('Write')?'Notes editor':'Summary editor'} disabled={editable===false} value={text} onChange={(e:any)=>{setText(e.target.value);const publish=()=>onChange({markdown:e.target.value,json:[]});if(delayConversion)pending.current=publish;else publish();}}/>;
}}));
const {default:PageContent}=await import(process.env.ISSUE14_PAGE_MODULE??'../../src/app/meeting-details/page-content');
let root:ReactTestRenderer;
function Page(){
 const [meeting,setMeeting]=useState({id:'m',title:'Meeting',created_at:'2026-11-18',transcripts:rows()});
 const [summary,setSummary]=useState<any>({markdown:'Stored summary'});
 return <PageContent meeting={meeting} summaryData={summary} onSummaryInvalidated={()=>setSummary(null)} onRefetchTranscripts={async()=>{setMeeting({...meeting,transcripts:rows()});}}/>;
}
const button=(text:string)=>root.root.findAllByType('button').find(n=>JSON.stringify(n.props.children).includes(text))!;
const editor=(label:string)=>root.root.findByProps({'aria-label':label});
afterEach(async()=>{await act(async()=>root?.unmount());timers.clear();reloads=0;savedNotes='';savedSummary='Stored summary';archivedSummary='';mutations=0;version=0;failSave=false;delayConversion=false;commands.length=0;});
for(const operation of ['select','correct'])test(`R1 normal page ${operation} flushes dirty editors before debounce and preserves later note edits`,async()=>{
 await act(async()=>{root=create(<Page/>);});
 delayConversion=operation==='select';
 const notes=editor('Notes editor');
 await act(async()=>{notes.props.onChange({target:{value:'Dirty notes'}});editor('Summary editor').props.onChange({target:{value:'Dirty summary'}});});
 expect(savedNotes).toBe('');expect(savedSummary).toBe('Stored summary');
 if(operation==='select'){
  await act(async()=>button('Review snapshot').props.onClick());
  await act(async()=>button('Save current corrections').props.onClick());
 }else{
  await act(async()=>button('Correct selected text').props.onClick());
  await act(async()=>button('Save text correction').props.onClick());
 }
 expect(reloads).toBe(0);expect(mutations).toBe(1);
 expect(savedNotes).toBe('Dirty notes');expect(archivedSummary).toBe('Dirty summary');
 expect(editor('Notes editor')).toBe(notes);
 expect(root.root.findAllByProps({'aria-label':'Summary editor'})).toHaveLength(0);
 expect(JSON.stringify(root.toJSON())).toContain('Selected text');
 delayConversion=false;
 await act(async()=>editor('Notes editor').props.onChange({target:{value:'Notes after switch'}}));
 await act(async()=>{for(const fn of [...timers.values()])await fn();});
 expect(savedNotes).toBe('Notes after switch');expect(savedSummary).toBe('');
});
test('R1 failed draft persistence blocks selection and keeps dirty editor for retry',async()=>{
 await act(async()=>{root=create(<Page/>);});
 await act(async()=>{editor('Summary editor').props.onChange({target:{value:'Keep draft'}});});
 await act(async()=>button('Review snapshot').props.onClick());
 failSave=true;
 await act(async()=>button('Save current corrections').props.onClick());
 expect(mutations).toBe(0);expect(editor('Summary editor').props.value).toBe('Keep draft');
 failSave=false;
 await act(async()=>button('Save current corrections').props.onClick());
 expect(mutations).toBe(1);expect(archivedSummary).toBe('Keep draft');
});
