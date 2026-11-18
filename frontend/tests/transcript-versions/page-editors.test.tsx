import React, {useState} from 'react';
import {act,create,ReactTestRenderer} from 'react-test-renderer';
import {afterEach,expect,mock,test} from 'bun:test';

// Only the rich-editor engine/view and native transport are mocked. PageContent,
// NotesTab, MeetingNotesEditor, NotesEditor, notes-store and version controls are real.
const timers = new Map<number,()=>void>(); let timerId=0, clock=0, reloads=0;
const storage = new Map<string,string>(); let quota=false;
const browserStorage = {
 get length(){return storage.size;}, key:(index:number)=>[...storage.keys()][index]??null,
 getItem:(key:string)=>storage.get(key)??null,
 setItem:(key:string,value:string)=>{if(quota)throw new DOMException('Full','QuotaExceededError');storage.set(key,value);},
 removeItem:(key:string)=>storage.delete(key), clear:()=>storage.clear(),
};
(globalThis as any).localStorage=browserStorage;
(globalThis as any).window={innerWidth:1200,matchMedia:()=>({matches:false,addEventListener(){},removeEventListener(){}}),addEventListener(){},removeEventListener(){},location:{reload(){reloads++;}},setTimeout:(fn:()=>void)=>{timers.set(++timerId,fn);return timerId;},clearTimeout:(id:number)=>timers.delete(id)};
const originalSetTimeout=globalThis.setTimeout, originalClearTimeout=globalThis.clearTimeout;
const storeTimers = new Map<number,()=>void>();
globalThis.setTimeout=((fn:any,delay:number,...args:any[])=>delay===400?(storeTimers.set(++timerId,fn),timerId):originalSetTimeout(fn,delay,...args)) as any;
globalThis.clearTimeout=((id:any)=>{if(storeTimers.delete(id))return;originalClearTimeout(id);}) as any;
process.on('exit',()=>{globalThis.setTimeout=originalSetTimeout;globalThis.clearTimeout=originalClearTimeout;});
const rich:any[]=[{id:'heading',type:'heading',props:{level:2},content:[{type:'text',text:'Заметки RU/EN',styles:{bold:true}}],children:[]},{id:'task',type:'bulletListItem',props:{},content:[{type:'text',text:'API follow-up',styles:{italic:true}}],children:[]}];
const richMarkdown='## **Заметки RU/EN**\n\n- *API follow-up*';
let seq=0, active='';
let savedSummary='Stored summary', archivedSummary='', mutations=0, failNotes=false, failSummary=false, version=0, pendingSummary=false;
let holdNotes=false, holdSummary=false, holdWrite=false, failNotesConversion=false;
const conversions:Array<{kind:string;resolve:(value:string)=>void}> = [];
let releaseWrite:(()=>void)|undefined;
const documents=new Map<string,any>();
const calls:Array<[string,any]>=[];
const rows=()=>[{id:version?'new-line':'old-line',text:version?'Selected text':'Live text',speaker:'You',audio_start_time:1,audio_end_time:2}];
const canonical=(target:string)=>target.startsWith('recording:/synthetic/')?`meeting:${target.slice('recording:/synthetic/'.length)}`:target;
const transport=async(name:string,args:any):Promise<any>=>{
 calls.push([name,structuredClone(args)]);
 if(name==='api_notes_recording_target')return `recording:/synthetic/${args.meetingId}`;
 if(name==='api_read_notes_document')return structuredClone(documents.get(canonical(args.target))??null);
 if(name==='api_write_notes_document'){
  if(holdWrite)await new Promise<void>(resolve=>{releaseWrite=resolve;});
  if(failNotes)throw new Error('Notes disk unavailable');
  const target=canonical(args.target), previous=documents.get(target);
  if((previous?.revision??0)!==args.expectedRevision)throw new Error('Notes conflict: competing writer');
  const revision=args.expectedRevision+1;
  documents.set(target,{markdown:args.markdown,json:structuredClone(args.json),revision,source:'user_notes'});
  return revision;
 }
 if(name==='list_transcript_versions')return {selected_id:version?'new':'live',revision:7+version,audio_available:true,versions:[{id:'new',kind:'refined',status:'ready',created_at:'2026-11-18',settings_json:'{"recording_metadata":{"in_person":false},"source_map":{"new-line":"microphone"}}'}]};
 if(name==='get_transcript_version')return [{id:'new-line',text:'Selected text',speaker:'You'}];
 if(name==='get_selected_transcript')return {version_id:version?'new':'live',revision:7+version,settings_json:'{"source_map":{"new-line":"microphone"}}',segments:rows()};
 if(name==='api_get_meeting_summary_language')return {language:null,storage:'metadata'};
 if(name==='api_save_meeting_summary'){if(failSummary)throw new Error('Summary storage unavailable');savedSummary=args.summary.markdown;return;}
 if(name==='select_transcript_version'||name==='correct_transcript_text'){
  if(pendingSummary)throw new Error('Wait for or cancel summary generation before selecting a transcript version');
  archivedSummary=savedSummary;savedSummary='';version++;mutations++;return;
 }
 throw new Error(name);
};
mock.module('@tauri-apps/api/event', () => ({ listen: async () => () => {} }));
mock.module('@tauri-apps/api/core',()=>({invoke:transport}));
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
mock.module('@/lib/workspace-api',()=>({getMeetingGroup:async()=>null,setMeetingGroup:async()=>{},announceChange(){},listActionItems:async()=>[],onWorkspaceChange:()=>()=>{},askMeeting:async()=>{}}));
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

mock.module('@blocknote/react',()=>({useCreateBlockNote:(options:any)=>{
 const [engine]=useState(()=>{
  const kind=options.placeholders.emptyDocument.startsWith('Your own Notes')?'notes':'summary';
  const listeners=new Set<()=>void>();
  return {kind,id:++timerId,document:structuredClone(options.initialContent??[]),
   onChange:(fn:()=>void)=>{listeners.add(fn);return()=>listeners.delete(fn);},
   change(blocks:any[]){this.document=structuredClone(blocks);listeners.forEach(fn=>fn());},
   tryParseMarkdownToBlocks:async(text:string)=>[{id:'loaded',type:'paragraph',content:[{type:'text',text,styles:{}}]}],
   replaceBlocks(_old:any,blocks:any){this.document=blocks;},
   blocksToMarkdownLossy:async(blocks:any[])=>{
    if(kind==='notes'&&failNotesConversion)throw new Error('Conversion failed');
    if((kind==='notes'&&holdNotes)||(kind==='summary'&&holdSummary))return new Promise<string>(resolve=>conversions.push({kind,resolve}));
    return JSON.stringify(blocks)===JSON.stringify(rich)?richMarkdown:blocks.map(b=>b.content.map((c:any)=>c.text).join('')).join('\n');
   },
  };
 }); return engine;
},GridSuggestionMenuController:()=>null,SuggestionMenuController:()=>null}));
mock.module('@blocknote/shadcn',()=>({BlockNoteView:({editor,editable}:any)=><textarea aria-label={editor.kind==='notes'?'Notes editor':'Summary editor'} data-engine={editor.id} disabled={!editable} value={JSON.stringify(editor.document)} onChange={(e:any)=>editor.change(e.target.blocks??[{id:'typed',type:'paragraph',content:[{type:'text',text:e.target.value,styles:{}}]}])}/>}));
mock.module('@/lib/app-theme',()=>({useAppTheme:()=>['light'],themeInfo:()=>({dark:false})}));
mock.module('@/components/editor/blocknote-menus',()=>({useBlockNoteLayer:()=>null,BlockNoteLayerProvider:({children}:any)=>children,blockNoteMenus:{}}));
const {default:PageContent}=await import('../../src/app/meeting-details/page-content');
const {createNotesStore,flushNotes,getNotesSnapshot,notesForContext}=await import('../../src/lib/notes-store');
let root:ReactTestRenderer|undefined;
function Page(){
 const [meeting,setMeeting]=useState({id:active,title:'Meeting',created_at:'2026-11-18',transcripts:rows()});
 const [summary,setSummary]=useState<any>({markdown:savedSummary});
 return <PageContent meeting={meeting} summaryData={summary} onSummaryInvalidated={()=>setSummary(null)} onRefetchTranscripts={async()=>{setMeeting({...meeting,transcripts:rows()});}}/>;
}
const button=(text:string)=>root!.root.findAllByType('button').find(n=>typeof n.props.children==='string'&&n.props.children.includes(text))!;
const editor=(label:string)=>root!.root.findByProps({'aria-label':label});
const ready=async()=>{await act(async()=>{for(const fn of [...timers.values()])fn();timers.clear();});};
async function mount(seed=true){active=`combined-${++seq}`;if(seed)documents.set(`meeting:${active}`,{markdown:'Stored notes',json:null,revision:1,source:'user_notes'});await act(async()=>{root=create(<Page/>);});await ready();await act(async()=>{await flushNotes(`meeting:${active}`);});}
async function review(operation:string){await act(async()=>button(operation==='select'?'Review snapshot':'Correct selected text').props.onClick());}
const mutate=(operation:string)=>button(operation==='select'?'Save current corrections':'Save text correction').props.onClick();
afterEach(async()=>{holdNotes=false;holdSummary=false;holdWrite=false;failNotes=false;failSummary=false;pendingSummary=false;failNotesConversion=false;releaseWrite?.();for(const p of conversions.splice(0))p.resolve(richMarkdown);await act(async()=>root?.unmount());root=undefined;timers.clear();storeTimers.clear();storage.clear();documents.clear();calls.length=0;reloads=0;mutations=0;version=0;quota=false;savedSummary='Stored summary';archivedSummary='';releaseWrite=undefined;clock=0;});

for(const operation of ['select','correct'])test(`combined real Notes/store survives formatted edit then ${operation} before 800ms with pending conversion`,async()=>{
 await mount();const notes=editor('Notes editor'),identity=notes.props['data-engine'];
 holdNotes=true;holdSummary=true;
 await act(async()=>{notes.props.onChange({target:{blocks:rich}});editor('Summary editor').props.onChange({target:{value:'Dirty summary'}});});
 clock=200;expect(clock).toBeLessThan(800);
 const target=`meeting:${active}`;
 expect(getNotesSnapshot(target).content.json).toEqual(rich);
 expect([...storage.values()].join('')).toContain('Заметки RU/EN');
 await review(operation);let action!:Promise<void>;
 await act(async()=>{action=mutate(operation);});
 expect(mutations).toBe(0);expect(editor('Notes editor').props.disabled).toBe(true);
 holdNotes=false;holdSummary=false;
 await act(async()=>{for(const p of conversions.splice(0))p.resolve(p.kind==='notes'?richMarkdown:'Dirty summary');await action;});
 expect(mutations).toBe(1);expect(reloads).toBe(0);
 expect(editor('Notes editor').props['data-engine']).toBe(identity);
 expect(editor('Notes editor').props.disabled).toBe(false);
 expect(documents.get(target).json).toEqual(rich);expect(documents.get(target).markdown).toBe(richMarkdown);
 expect(archivedSummary).toBe('Dirty summary');expect(savedSummary).toBe('');
 expect(root!.root.findAllByProps({'aria-label':'Summary editor'})).toHaveLength(0);
 expect(JSON.stringify(root!.toJSON())).toContain('Selected text');
 expect(calls.filter(([name])=>name==='api_write_notes_document').every(([,args])=>args.target===target)).toBe(true);
 const context=await notesForContext(target);expect(context.source).toBe('user_notes');expect(context.json).toEqual(rich);
 await act(async()=>root!.unmount());await act(async()=>{root=create(<Page/>);});await ready();
 expect(JSON.parse(editor('Notes editor').props.value)).toEqual(rich);
 const restarted=createNotesStore({invoke:transport as any,storage:browserStorage as Storage});
 await restarted.loadNotes(target);expect((await restarted.notesForContext(target)).json).toEqual(rich);
});

test('combined selection waits for the real Notes native write already in flight',async()=>{
 await mount();await act(async()=>editor('Notes editor').props.onChange({target:{blocks:rich}}));
 holdWrite=true;let saving!:Promise<void>;await act(async()=>{saving=flushNotes(`meeting:${active}`);});
 await review('select');let action!:Promise<void>;await act(async()=>{action=mutate('select');});
 expect(mutations).toBe(0);expect(getNotesSnapshot(`meeting:${active}`).state).toBe('saving');
 holdWrite=false;await act(async()=>{releaseWrite!();await saving;await action;});
 expect(mutations).toBe(1);expect(documents.get(`meeting:${active}`).json).toEqual(rich);
});

test('combined Notes failure retains format, Retry commits, then selection succeeds',async()=>{
 await mount();await act(async()=>editor('Notes editor').props.onChange({target:{blocks:rich}}));
 failNotes=true;await review('select');await act(async()=>mutate('select'));
 expect(mutations).toBe(0);expect(JSON.stringify(root!.toJSON())).toContain('Notes disk unavailable');
 expect(getNotesSnapshot(`meeting:${active}`).content.json).toEqual(rich);
 failNotes=false;await act(async()=>button('Retry').props.onClick());await ready();
 await act(async()=>mutate('select'));expect(mutations).toBe(1);
 expect(documents.get(`meeting:${active}`).markdown).toBe(richMarkdown);
});

test('combined competing Notes writer is not overwritten by selector; reviewed replacement is explicit',async()=>{
 await mount();const target=`meeting:${active}`;
 const competitor=createNotesStore({invoke:transport as any,storage:browserStorage as Storage});await competitor.loadNotes(target);
 competitor.editNotes(target,{markdown:'Other writer',json:[]},Promise.resolve('Other writer'));await Promise.resolve();await competitor.flushNotes(target);
 await act(async()=>editor('Notes editor').props.onChange({target:{blocks:rich}}));
 await review('select');await act(async()=>mutate('select'));
 expect(mutations).toBe(0);expect(documents.get(target).markdown).toBe('Other writer');
 expect(getNotesSnapshot(target).content.json).toEqual(rich);
 expect([...storage.values()].join('')).toContain('Заметки RU/EN');
 await act(async()=>button('Review saved version').props.onClick());
 await act(async()=>button('Replace reviewed version with my draft').props.onClick());
 await act(async()=>mutate('select'));expect(mutations).toBe(1);expect(documents.get(target).json).toEqual(rich);
});

test('combined native pending summary rejects selection without replacing dirty Notes',async()=>{
 await mount();await act(async()=>editor('Notes editor').props.onChange({target:{blocks:rich}}));
 pendingSummary=true;await review('select');await act(async()=>mutate('select'));
 expect(mutations).toBe(0);expect(documents.get(`meeting:${active}`).json).toEqual(rich);
 expect(JSON.stringify(root!.toJSON())).toContain('Wait for or cancel summary');
 pendingSummary=false;await act(async()=>mutate('select'));expect(mutations).toBe(1);
 expect(documents.get(`meeting:${active}`).json).toEqual(rich);
});

test('combined read-only quota recovery stays locked across select and requires explicit Notes save',async()=>{
 active=`combined-${++seq}`;const target=`meeting:${active}`;
 const key=`af-notes-v2:draft:${encodeURIComponent(target)}:retained`;
 storage.set(key,JSON.stringify({target,content:{markdown:richMarkdown,json:rich},revision:0,converting:false}));quota=true;
 await act(async()=>{root=create(<Page/>);});await ready();
 expect(editor('Notes editor').props.disabled).toBe(true);expect(storage.has(key)).toBe(true);
 await review('select');await act(async()=>mutate('select'));
 expect(mutations).toBe(0);expect(storage.has(key)).toBe(true);
 expect(calls.filter(([name])=>name==='api_write_notes_document')).toHaveLength(0);
 await act(async()=>button('Save retained Notes to meeting').props.onClick());
 await act(async()=>mutate('select'));
 expect(mutations).toBe(1);expect(editor('Notes editor').props.disabled).toBe(true);
 expect(documents.get(target).json).toEqual(rich);expect(documents.get(target).source).toBe('user_notes');
});

test('combined distinct recording/meeting drafts require a choice and retain the other alias',async()=>{
 active=`combined-${++seq}`;const target=`meeting:${active}`,alias=`recording:/synthetic/${active}`;
 const own=`af-notes-v2:draft:${encodeURIComponent(target)}:own`,other=`af-notes-v2:draft:${encodeURIComponent(alias)}:other`;
 storage.set(own,JSON.stringify({target,content:{markdown:richMarkdown,json:rich},revision:0,converting:false}));
 const otherRaw=JSON.stringify({target:alias,content:{markdown:'Other alias',json:[]},revision:0,converting:false});storage.set(other,otherRaw);
 await act(async()=>{root=create(<Page/>);});await review('select');await act(async()=>mutate('select'));
 expect(mutations).toBe(0);expect(storage.get(other)).toBe(otherRaw);
 await act(async()=>button('Open this draft').props.onClick());await ready();await act(async()=>mutate('select'));
 expect(mutations).toBe(1);expect(documents.get(target).json).toEqual(rich);expect(storage.get(other)).toBe(otherRaw);
 const fresh=createNotesStore({invoke:transport as any,storage:browserStorage as Storage});await fresh.loadNotes(target);
 expect(fresh.getNotesSnapshot(target).content.markdown).toBe('Other alias');
});

 test('combined failed conversion retains exact Notes JSON and requires Retry before correction',async()=>{
  await mount();failNotesConversion=true;
  await act(async()=>editor('Notes editor').props.onChange({target:{blocks:rich}}));
  await review('correct');await act(async()=>mutate('correct'));
  expect(mutations).toBe(0);expect(getNotesSnapshot(`meeting:${active}`).content.json).toEqual(rich);
  failNotesConversion=false;await act(async()=>button('Retry').props.onClick());await ready();
  await act(async()=>mutate('correct'));expect(mutations).toBe(1);expect(documents.get(`meeting:${active}`).json).toEqual(rich);
 });
 test('combined failed summary persistence blocks selection without overwriting Notes',async()=>{
  await mount();await act(async()=>{editor('Notes editor').props.onChange({target:{blocks:rich}});editor('Summary editor').props.onChange({target:{value:'Dirty summary'}});});
  failSummary=true;await review('select');await act(async()=>mutate('select'));
  expect(mutations).toBe(0);expect(documents.get(`meeting:${active}`).json).toEqual(rich);
  expect(JSON.stringify(root!.toJSON())).toContain('Summary editor');
  failSummary=false;await act(async()=>mutate('select'));
  expect(mutations).toBe(1);expect(archivedSummary).toBe('Dirty summary');expect(documents.get(`meeting:${active}`).json).toEqual(rich);
 });
