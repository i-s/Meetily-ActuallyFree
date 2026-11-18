import React, {useState} from 'react';
import {act,create,ReactTestRenderer} from 'react-test-renderer';
import {afterEach,expect,mock,spyOn,test} from 'bun:test';

// Combined normal callers are real: PageContent, version controls, rich Notes
// editor/store, library/search hook, detail and library export dialogs/pipeline.
// Native transport, BlockNote view/engine, picker/fs and unrelated chrome are adapters.
const timers = new Map<number,()=>void>(); let timerId=0, clock=0, reloads=0;
const storage = new Map<string,string>(); let quota=false;
const browserStorage = {
 get length(){return storage.size;}, key:(index:number)=>[...storage.keys()][index]??null,
 getItem:(key:string)=>storage.get(key)??null,
 setItem:(key:string,value:string)=>{if(quota)throw new DOMException('Full','QuotaExceededError');storage.set(key,value);},
 removeItem:(key:string)=>storage.delete(key), clear:()=>storage.clear(),
};
(globalThis as any).localStorage=browserStorage;
(globalThis as any).window={innerWidth:1200,matchMedia:()=>({matches:false,addEventListener(){},removeEventListener(){}}),addEventListener(){},removeEventListener(){},location:{search:'',pathname:'/meetings',reload(){reloads++;}},history:{replaceState(){}},atob:globalThis.atob,btoa:globalThis.btoa,setTimeout:(fn:()=>void)=>{timers.set(++timerId,fn);return timerId;},clearTimeout:(id:number)=>timers.delete(id)};
const originalSetTimeout=globalThis.setTimeout, originalClearTimeout=globalThis.clearTimeout;
const storeTimers = new Map<number,()=>void>();
globalThis.setTimeout=((fn:any,delay:number,...args:any[])=>(delay===400||delay===220)?(storeTimers.set(++timerId,fn),timerId):originalSetTimeout(fn,delay,...args)) as any;
globalThis.clearTimeout=((id:any)=>{if(storeTimers.delete(id))return;originalClearTimeout(id);}) as any;
process.on('exit',()=>{globalThis.setTimeout=originalSetTimeout;globalThis.clearTimeout=originalClearTimeout;});
const rich:any[]=[{id:'heading',type:'heading',props:{level:2},content:[{type:'text',text:'Заметки RU/EN',styles:{bold:true}}],children:[]},{id:'task',type:'bulletListItem',props:{},content:[{type:'text',text:'API follow-up',styles:{italic:true}}],children:[]}];
const richMarkdown='## **Заметки RU/EN**\n\n- *API follow-up*';
let seq=0, active='';
let selectedText='Live text',selectedRowId='old-line';
let outputPath:string|null='/synthetic/user-selected.custom',writeError=false,pickerWait:Promise<void>|null=null;
const written:Array<{path:string;text:string}>=[],toasts:Array<{kind:string;text:string}>=[];
let savedSummary='Stored summary', archivedSummary='', mutations=0, failNotes=false, failSummary=false, version=0, pendingSummary=false;
let holdNotes=false, holdSummary=false, holdWrite=false, failNotesConversion=false;
const conversions:Array<{kind:string;resolve:(value:string)=>void}> = [];
let releaseWrite:(()=>void)|undefined;
let actionWait:Promise<void>|null=null, actionReplyWait:Promise<void>|null=null, actionStarted=false, summaryToken=0, summaryExists=true;
let actionRows:any[]=[];
let canonicalSummary:any=null, canonicalWait:Promise<void>|null=null;
const nativeEvents=new Set<()=>void>();
mock.module('@tauri-apps/api/event',()=>({listen:async(_name:string,fn:()=>void)=>{nativeEvents.add(fn);return()=>nativeEvents.delete(fn);}}));
const documents=new Map<string,any>();
const calls:Array<[string,any]>=[];
const rows=()=>[{id:selectedRowId,text:selectedText,speaker:'You',audio_start_time:1,audio_end_time:2}];
const canonical=(target:string)=>target.startsWith('recording:/synthetic/')?`meeting:${target.slice('recording:/synthetic/'.length)}`:target;
const transport=async(name:string,args:any):Promise<any>=>{
 calls.push([name,structuredClone(args)]);
 if(name==='post_call_canonical_summary'){const snapshot=structuredClone(canonicalSummary);if(canonicalWait)await canonicalWait;return snapshot;}
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
 if(name==='api_action_sync_context')return !summaryExists?null:{meetingId:args.meetingId,versionId:version?'new':'live',revision:7+version,summaryToken:String(summaryToken),summary:{markdown:savedSummary},transcripts:rows()};
 if(name==='api_sync_ai_action_items'||name==='api_commit_action_sync'){
  actionStarted=true;if(actionWait)await actionWait;
  if(name==='api_commit_action_sync'){
   if(args.context.revision!==7+version||args.context.summaryToken!==String(summaryToken))return null;
   if(args.summary){savedSummary=args.summary.markdown;summaryToken++;}
  }
  if(actionReplyWait)await actionReplyWait;
  return [];
 }
 if(name==='api_get_meeting_summary_language')return {language:null,storage:'metadata'};
 if(name==='api_save_meeting_summary'){if(failSummary)throw new Error('Summary storage unavailable');if(summaryExists){savedSummary=args.summary.markdown;summaryToken++;}return;}
 if(name==='select_transcript_version'||name==='correct_transcript_text'){
  if(pendingSummary)throw new Error('Wait for or cancel summary generation before selecting a transcript version');
  archivedSummary=savedSummary;savedSummary='';summaryExists=false;summaryToken++;version++;mutations++;if(name==='select_transcript_version'){selectedText='Selected text';selectedRowId='new-line';}else selectedText=args.text;return;
 }
 if(name==='api_export_meeting_snapshot'){
  const notes=structuredClone(documents.get(`meeting:${args.meetingId}`)??{markdown:'',json:null,revision:0});
  if(notes.revision!==args.expectedNotesRevision)throw new Error('Notes changed while capturing export');
  return {id:args.meetingId,title:'Meeting',createdAt:'2026-11-18',capturedAt:'2026-11-18T10:00:00Z',audioAvailable:false,notes:{...notes,source:'user_notes'},summary:savedSummary?JSON.stringify(canonicalSummary??{markdown:savedSummary}):null,summaryStatus:savedSummary?'completed':null,transcripts:rows().map(row=>({id:row.id,text:row.text,speaker:row.speaker,audioStartTime:row.audio_start_time})),actions:actionRows};
 }
 if(name==='api_library_materials')return [{id:active,transcriptCount:rows().length,hasNotes:documents.has(`meeting:${active}`),hasSummary:!!savedSummary,summaryStatus:savedSummary?'completed':null,audioAvailable:false}];
 if(name==='api_search_library'){
  const needle=args.query.toLowerCase(),notes=documents.get(`meeting:${active}`),hits:any[]=[];
  if(needle==='api'&&notes?.markdown===richMarkdown)hits.push({kind:'notes',id:`notes-${active}`,meetingId:active,title:'Meeting',snippet:'Заметки RU/EN\nAPI follow-up'});
  if(!hits.length&&savedSummary.toLowerCase().includes(needle))hits.push({kind:'summary',id:`summary-${active}`,meetingId:active,title:'Meeting',snippet:savedSummary});
  if(!hits.length&&selectedText.toLowerCase().includes(needle))hits.push({kind:'transcript',id:selectedRowId,transcriptId:selectedRowId,meetingId:active,title:'Meeting',snippet:selectedText,audioStartTime:1});
  return hits;
 }
 throw new Error(name);
};
mock.module('@tauri-apps/api/core',()=>({invoke:transport}));
const empty=()=>null;
mock.module('lucide-react',()=>Object.fromEntries(['AlertTriangle','ChevronDown','FileText','Languages','ListChecks','NotebookPen','RefreshCw','Settings2','Sparkles','Square','Wand2','Clipboard','FileJson','FileType','Files','ScrollText','ArrowUpRight','Check','Download','FileAudio','FolderInput','Layers','Library','MoreHorizontal','Pencil','Plus','Search','Trash2','X'].map(name=>[name,empty])));
const wrap=({children}:any)=><div>{children}</div>;
mock.module('framer-motion',()=>({motion:{div:wrap}}));
mock.module('sonner',()=>({toast:{success:(text:string)=>toasts.push({kind:'success',text}),error:(text:string,extra?:any)=>toasts.push({kind:'error',text:`${text}: ${extra?.description??''}`}),warning(){}}}));
mock.module('@tauri-apps/plugin-dialog',()=>({save:async()=>{if(pickerWait)await pickerWait;return outputPath;}}));
mock.module('@tauri-apps/plugin-fs',()=>({writeFile:async(path:string,bytes:Uint8Array)=>{if(writeError)throw new Error('Picker write denied');written.push({path,text:new TextDecoder().decode(bytes)});}}));
mock.module('@/lib/meeting-actions',()=>({deleteMeetings:async()=>{},renameMeeting:async()=>true,moveMeetingsToGroup:async()=>true}));
mock.module('@/lib/analytics',()=>({default:{trackPageView(){}}}));
const sidebar={get meetings(){return [{id:active,title:'Meeting',created_at:'2026-11-18'}];},currentMeeting:null,meetingsError:null,setCurrentMeeting(){},refetchMeetings:async()=>{}};
mock.module('@/components/Sidebar/SidebarProvider',()=>({useSidebar:()=>sidebar}));
mock.module('@/contexts/ConfigContext',()=>({useConfig:()=>({modelConfig:{model:'test'},setModelConfig(){}})}));
mock.module('@/contexts/WorkspaceContext',()=>({useWorkspace:()=>({people:[],groups:[],groupById:()=>null})}));
mock.module('@/contexts/ImportDialogContext',()=>({useImportDialog:()=>({openImportDialog(){}})}));
let navigated='';mock.module('next/navigation',()=>({useRouter:()=>({push:(href:string)=>{navigated=href;}})}));
mock.module('@/lib/workspace-api',()=>({getMeetingGroup:async()=>null,setMeetingGroup:async()=>{},announceChange(){},listActionItems:async()=>structuredClone(actionRows),onWorkspaceChange:()=>()=>{},askMeeting:async()=>{},syncAiActionItems:(meetingId:string,items:any,sourceFingerprint:string)=>transport('api_sync_ai_action_items',{meetingId,items,sourceFingerprint}),listUnsyncedActionMeetings:async()=>[]}));
mock.module('@/hooks/meeting-details/useSummaryGeneration',()=>({useSummaryGeneration:()=>({summaryStatus:'idle',summaryError:null,getSummaryStatusMessage:()=>'',handleGenerateSummary:async()=>{},handleRegenerateSummary:async()=>{},handleStopGeneration(){}})}));
mock.module('@/hooks/meeting-details/useTemplates',()=>({useTemplates:()=>({selectedTemplate:'standard',availableTemplates:[]})}));
mock.module('@/hooks/meeting-details/useMeetingOperations',()=>({useMeetingOperations:()=>({})}));
mock.module('@/hooks/useMeetingAudio',()=>({PLAYBACK_RATES:[],SLOW_PLAYBACK_RATES:[],useMeetingAudio:()=>({status:'unavailable',seek(){}})}));
mock.module('@/hooks/useWaveform',()=>({useWaveform:()=>null}));
mock.module('@/hooks/useLabs',()=>({useLabs:()=>({labs:{},ready:true})}));
mock.module('@/hooks/useUserName',()=>({useUserName:()=>''}));
mock.module('@/components/meeting/MeetingHeader',()=>({MeetingHeader:({onExport}:any)=><button onClick={onExport}>Open meeting export</button>}));
mock.module('@/components/meeting/AudioPlayerBar',()=>({AudioPlayerBar:empty}));
mock.module('@/components/VirtualizedTranscriptView',()=>({VirtualizedTranscriptView:({segments}:any)=><pre>{JSON.stringify(segments)}</pre>}));
mock.module('@/components/MeetingDetails/PostCallProcessingDialog',()=>({PostCallProcessingDialog:empty}));
mock.module('@/components/MeetingDetails/TemplateEditorModal',()=>({TemplateEditorModal:empty}));
mock.module('@/components/people/PersonCard',()=>({PersonCard:empty}));
mock.module('@/components/people/SpeakerIdentityDialog',()=>({SpeakerIdentityDialog:empty}));
mock.module('@/components/actions/ActionItemsList',()=>({ActionItemsList:({items,onItemsChange}:any)=><pre aria-label="Action rows" onClick={()=>onItemsChange(actionRows)}>{JSON.stringify(items)}</pre>}));
mock.module('@/components/chat/ChatThread',()=>({ChatThread:empty}));
mock.module('@/components/LanguagePickerPopover',()=>({LanguagePickerPopover:empty}));
mock.module('@/components/ModelSettingsModal',()=>({ModelSettingsModal:empty}));
mock.module('@/components/ui/button',()=>({Button:({children,loading,...p}:any)=><button {...p}>{children}</button>}));
mock.module('@/components/ui/surface',()=>({Skeleton:empty,Overline:wrap,PageHeader:({title}:any)=><h1>{title}</h1>,EmptyState:({title}:any)=><p>{title}</p>}));
for(const [path,names] of Object.entries({tabs:['Tabs','TabsContent','TabsList','TabsTrigger'],popover:['Popover','PopoverContent','PopoverTrigger'],select:['Select','SelectContent','SelectItem','SelectTrigger','SelectValue'],dialog:['Dialog','DialogContent','DialogDescription','DialogFooter','DialogHeader','DialogTitle']}))mock.module(`@/components/ui/${path}`,()=>Object.fromEntries(names.map(name=>[name,name==='Dialog'?({open,children}:any)=>open?<div>{children}</div>:null:wrap])));

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

mock.module('@/components/ui/input',()=>({Input:(props:any)=><input {...props}/>,fieldClass:''}));
mock.module('@/components/ui/checkbox',()=>({Checkbox:(props:any)=><input type="checkbox" {...props}/>}));
mock.module('@/components/ui/combobox',()=>({Combobox:empty}));
mock.module('@/components/ui/confirm-dialog',()=>({ConfirmDialog:empty}));
mock.module('@/components/ui/avatar',()=>({Avatar:empty}));
mock.module('@/components/ui/dropdown-menu',()=>Object.fromEntries(['DropdownMenu','DropdownMenuContent','DropdownMenuItem','DropdownMenuSeparator','DropdownMenuSub','DropdownMenuSubContent','DropdownMenuSubTrigger','DropdownMenuTrigger'].map(name=>[name,name==='DropdownMenuItem'?({onSelect,children}:any)=><button onClick={onSelect}>{children}</button>:wrap])));
mock.module('@/components/groups/GroupBits',()=>({createGroupFromPicker(){},GroupChip:empty,GroupDot:empty,groupOptions:()=>[]}));
mock.module('@/components/groups/GroupEditor',()=>({openGroupEditor(){}}));

const {default:MeetingsPage}=await import('../../src/app/meetings/page');
const {default:PageContent}=await import('../../src/app/meeting-details/page-content');
const {createNotesStore,flushNotes,getNotesSnapshot,notesForContext}=await import('../../src/lib/notes-store');
let root:ReactTestRenderer|undefined;
let navigateView:(id:string,summary:string)=>void;
function Page(){
 const [meeting,setMeeting]=useState({id:active,title:'Meeting',created_at:'2026-11-18',transcripts:rows()});
 const [summary,setSummary]=useState<any>({markdown:savedSummary});
 navigateView=(id,markdown)=>{setMeeting({id,title:'Meeting',created_at:'2026-11-18',transcripts:rows()});setSummary({markdown});};
 return <PageContent meeting={meeting} summaryData={summary} onSummaryInvalidated={()=>setSummary(null)} onRefetchTranscripts={async()=>{setMeeting({...meeting,transcripts:rows()});}}/>;
}
const button=(text:string)=>root!.root.findAllByType('button').find(n=>typeof n.props.children==='string'&&n.props.children.includes(text))!;
const editor=(label:string)=>root!.root.findByProps({'aria-label':label});
const ready=async()=>{await act(async()=>{for(const fn of [...timers.values()])fn();timers.clear();});};
async function mount(seed=true){active=`combined-${++seq}`;if(seed)documents.set(`meeting:${active}`,{markdown:'Stored notes',json:null,revision:1,source:'user_notes'});await act(async()=>{root=create(<Page/>);});await ready();await act(async()=>{await flushNotes(`meeting:${active}`);});}
async function review(operation:string){await act(async()=>button(operation==='select'?'Review snapshot':'Correct selected text').props.onClick());}
const mutate=(operation:string)=>button(operation==='select'?'Save current corrections':'Save text correction').props.onClick();
afterEach(async()=>{holdNotes=false;holdSummary=false;holdWrite=false;failNotes=false;failSummary=false;pendingSummary=false;failNotesConversion=false;releaseWrite?.();for(const p of conversions.splice(0))p.resolve(richMarkdown);await act(async()=>root?.unmount());root=undefined;timers.clear();storeTimers.clear();storage.clear();documents.clear();calls.length=0;reloads=0;mutations=0;version=0;selectedText='Live text';selectedRowId='old-line';written.length=0;toasts.length=0;outputPath='/synthetic/user-selected.custom';writeError=false;pickerWait=null;navigated='';quota=false;savedSummary='Stored summary';archivedSummary='';releaseWrite=undefined;clock=0;actionWait=null;actionReplyWait=null;actionStarted=false;summaryToken=0;summaryExists=true;actionRows=[];canonicalSummary=null;canonicalWait=null;nativeEvents.clear();});

const visible=(node:any):string=>typeof node==='string'?node:Array.isArray(node)?node.map(visible).join(''):(node?.children??[]).map(visible).join('');
const exactButton=(label:string,last=false)=>{const found=root!.root.findAllByType('button').filter(node=>visible(node)===label);return last?found.at(-1)!:found[0]!;};
const markdownButton=()=>root!.root.findAllByType('button').find(node=>visible(node).startsWith('Markdown')&&visible(node).includes('Formatted .md'))!;
async function detailExport(){await act(async()=>exactButton('Open meeting export').props.onClick());await act(async()=>exactButton('Choose format').props.onClick());await act(async()=>markdownButton().props.onClick());}
async function search(query:string){await act(async()=>root!.root.findByProps({'aria-label':'Filter meetings'}).props.onChange({target:{value:query}}));await act(async()=>{const pending=[...storeTimers.values()].map(fn=>fn());storeTimers.clear();await Promise.all(pending);});}

for(const operation of ['select','correct'])test(`combined actual detail/version/rich Notes/library callers retain immediate formatting through ${operation} and both exports`,async()=>{
 await mount();const target=`meeting:${active}`,identity=editor('Notes editor').props['data-engine'],before=documents.get(target).revision;
 holdNotes=true;holdSummary=true;
 await act(async()=>{editor('Notes editor').props.onChange({target:{blocks:rich}});editor('Summary editor').props.onChange({target:{value:'Dirty summary'}});});
 await review(operation);
 if(operation==='correct')await act(async()=>editor('Corrected transcript text').props.onChange({target:{value:'Corrected API words'}}));
 let action!:Promise<void>;await act(async()=>{action=mutate(operation);});
 expect(mutations).toBe(0);expect(editor('Notes editor').props.disabled).toBe(true);
 holdNotes=false;holdSummary=false;
 await act(async()=>{for(const item of conversions.splice(0))item.resolve(item.kind==='notes'?richMarkdown:'Dirty summary');await action;});
 const chosen=operation==='select'?'Selected text':'Corrected API words',revision=before+1;
 expect(mutations).toBe(1);expect(selectedText).toBe(chosen);expect(editor('Notes editor').props['data-engine']).toBe(identity);
 expect(documents.get(target).json).toEqual(rich);expect(documents.get(target).revision).toBe(revision);
 expect(archivedSummary).toBe('Dirty summary');expect(savedSummary).toBe('');expect(root!.root.findAllByProps({'aria-label':'Summary editor'})).toHaveLength(0);
 await detailExport();
 expect(written).toHaveLength(1);expect(written[0].path).toBe('/synthetic/user-selected.custom');
 for(const expected of [chosen,richMarkdown,`Revision: ${revision}.`,'user_notes','spoken_transcript','No summary available'])expect(written[0].text).toContain(expected);
 expect(written[0].text).not.toContain('Dirty summary');expect(written[0].text).not.toContain('Stored summary');
 await act(async()=>root!.update(<MeetingsPage/>));
 await search('API');expect(visible(root!.toJSON())).toContain('Your Notes (not speech)');expect(visible(root!.toJSON())).toContain('API follow-up');
 await act(async()=>root!.root.findByType('li').props.onClick({}));expect(navigated).toContain(`id=${active}&section=notes`);
 await act(async()=>exactButton('Export…').props.onClick());
 await act(async()=>exactButton('Export',true).props.onClick());
 expect(written).toHaveLength(2);expect(written[1].text).toContain(chosen);expect(written[1].text).toContain(`Revision: ${revision}.`);expect(written[1].text).not.toContain('Dirty summary');
 await search(operation==='select'?'Selected':'Corrected');
 expect(visible(root!.toJSON())).toContain(chosen);
 await act(async()=>root!.root.findByType('li').props.onClick({}));expect(navigated).toContain(`t=${selectedRowId}&ts=1`);
 await search('Dirty summary');expect(root!.root.findAllByType('li')).toHaveLength(0);
 expect(documents.get(target).revision).toBe(revision);
},60000);

test('combined dirty Notes conflict blocks version mutation and actual detail export until explicit reviewed save',async()=>{
 await mount();const target=`meeting:${active}`;
 const other=createNotesStore({invoke:transport as any,storage:browserStorage as Storage});await other.loadNotes(target);
 other.editNotes(target,{markdown:'Other writer',json:[]},Promise.resolve('Other writer'));await Promise.resolve();await other.flushNotes(target);
 await act(async()=>editor('Notes editor').props.onChange({target:{blocks:rich}}));
 await review('select');await act(async()=>mutate('select'));
 expect(mutations).toBe(0);expect(visible(root!.toJSON())).toContain('Notes conflict');expect(getNotesSnapshot(target).content.json).toEqual(rich);
 await detailExport();expect(written).toHaveLength(0);expect(toasts.at(-1)?.text).toContain('Notes conflict');expect(documents.get(target).markdown).toBe('Other writer');
 // The normal conflict controls require review before replacing another writer.
 await act(async()=>button('Review saved version').props.onClick());await act(async()=>button('Replace reviewed version with my draft').props.onClick());
 await act(async()=>mutate('select'));expect(mutations).toBe(1);expect(documents.get(target).json).toEqual(rich);
 // The existing export dialog is still at its format step; retry, not a second builder.
 await act(async()=>markdownButton().props.onClick());expect(written).toHaveLength(1);expect(written[0].text).toContain(richMarkdown);expect(written[0].text).not.toContain('Other writer');
},60000);

test('combined picker failure/cancellation and navigation cannot write an abandoned meeting snapshot',async()=>{
 await mount();await act(async()=>editor('Notes editor').props.onChange({target:{blocks:rich}}));await review('select');await act(async()=>mutate('select'));
 writeError=true;await detailExport();expect(written).toHaveLength(0);expect(toasts.at(-1)?.text).toContain('Picker write denied');
 writeError=false;outputPath=null;const successes=toasts.filter(item=>item.kind==='success').length;
 await act(async()=>markdownButton().props.onClick());expect(written).toHaveLength(0);expect(toasts.filter(item=>item.kind==='success')).toHaveLength(successes);
 outputPath='/synthetic/user-selected.custom';let release!:()=>void;pickerWait=new Promise<void>(resolve=>{release=resolve;});
 await act(async()=>markdownButton().props.onClick());
 const oldTarget=`meeting:${active}`;active=`combined-${++seq}`;documents.set(`meeting:${active}`,{markdown:'Meeting B Notes',json:null,revision:1,source:'user_notes'});
 await act(async()=>{root!.update(<Page key={active}/>);release();});await ready();
 expect(written).toHaveLength(0);expect(documents.get(oldTarget).json).toEqual(rich);expect(toasts.filter(item=>item.kind==='success')).toHaveLength(successes);
},60000);

test('combined detail siblings have distinct keys while retaining meeting-owned editors',async()=>{
 const errors:any[][]=[];const error=spyOn(console,'error').mockImplementation((...args:any[])=>{errors.push(args);});
 try{await mount();expect(errors.filter(args=>String(args[0]).includes('same key'))).toEqual([]);}
 finally{error.mockRestore();}
});


for(const operation of ['select','correct'])test(`delayed real action extraction cannot restore summary after external ${operation}`,async()=>{
 savedSummary='## Overview\nOld summary\n\n## Action items\n- Send API report';
 let release!:()=>void;actionWait=new Promise<void>(resolve=>{release=resolve;});
 await mount();expect(actionStarted).toBe(true);
 // Another WebView can change the projection while this one's native call waits.
 await act(async()=>{await transport(operation==='select'?'select_transcript_version':'correct_transcript_text',{text:'Corrected API'});const selector=root!.root.findAll(node=>typeof node.type==='function'&&node.type.name==='TranscriptVersions')[0];await selector.props.onChanged();});
 await act(async()=>{release();});
 expect(savedSummary).toBe('');
 expect(root!.root.findAllByProps({'aria-label':'Summary editor'})).toHaveLength(0);
 const {captureMeetingExport}=await import('../../src/lib/meeting-export');
 expect((await captureMeetingExport(active)).summary).toBeNull();
},60000);

test('delayed real extraction cannot overwrite a new same-version summary or unsaved editor draft',async()=>{
 savedSummary='## Overview\nOld summary\n\n## Action items\n- Send API report';
 let release!:()=>void;actionWait=new Promise<void>(resolve=>{release=resolve;});
 await mount();expect(actionStarted).toBe(true);
 await act(async()=>editor('Summary editor').props.onChange({target:{value:'New unsaved draft'}}));
 savedSummary='New generated summary';summaryToken++;
 await act(async()=>{release();});
 expect(savedSummary).toBe('New generated summary');
 expect(editor('Summary editor').props.value).toContain('New unsaved draft');
},60000);

for(const operation of ['select','correct'])test(`local ${operation} drains pending real extraction before archiving summary`,async()=>{
 savedSummary='## Overview\nOld summary\n\n## Action items\n- Send API report';
 let release!:()=>void;actionWait=new Promise<void>(resolve=>{release=resolve;});
 await mount();expect(actionStarted).toBe(true);
 await act(async()=>editor('Notes editor').props.onChange({target:{blocks:rich}}));
 const identity=editor('Notes editor').props['data-engine'];
 await review(operation);if(operation==='correct')await act(async()=>editor('Corrected transcript text').props.onChange({target:{value:'Corrected API'}}));
 let work!:Promise<void>;await act(async()=>{work=mutate(operation);});
 expect(mutations).toBe(0);
 await act(async()=>{release();await work;});
 expect(mutations).toBe(1);expect(archivedSummary).toBe('## Overview\nOld summary');expect(savedSummary).toBe('');
 expect(editor('Notes editor').props['data-engine']).toBe(identity);expect(documents.get(`meeting:${active}`).json).toEqual(rich);
 expect(root!.root.findAllByProps({'aria-label':'Summary editor'})).toHaveLength(0);
},60000);

test('same-version real extraction updates summary UI and export on ordinary success',async()=>{
 savedSummary='## Overview\nOld summary\n\n## Action items\n- Send API report';
 let release!:()=>void;actionWait=new Promise<void>(resolve=>{release=resolve;});
 await mount();expect(actionStarted).toBe(true);
 await act(async()=>{release();});await ready();
 expect(savedSummary).toBe('## Overview\nOld summary');
 expect(editor('Summary editor').props.value).not.toContain('Action items');
 const {captureMeetingExport}=await import('../../src/lib/meeting-export');
 expect(JSON.parse((await captureMeetingExport(active)).summary!).markdown).toBe(savedSummary);
},60000);

test('successful extraction cannot replace a newer rich draft still awaiting Markdown conversion',async()=>{
 savedSummary='## Overview\nOld summary\n\n## Action items\n- Send API report';
 let release!:()=>void;actionWait=new Promise<void>(resolve=>{release=resolve;});
 await mount();expect(actionStarted).toBe(true);
 const identity=editor('Summary editor').props['data-engine'];holdSummary=true;
 await act(async()=>editor('Summary editor').props.onChange({target:{blocks:rich}}));
 await act(async()=>{release();root!.update(<Page/>);});
 expect(savedSummary).toBe('## Overview\nOld summary');
 expect(editor('Summary editor').props['data-engine']).toBe(identity);
 expect(editor('Summary editor').props.value).toContain('API follow-up');
 holdSummary=false;await act(async()=>{for(const item of conversions.splice(0))item.resolve(richMarkdown);});
 expect(editor('Summary editor').props['data-engine']).toBe(identity);
},60000);


for(const operation of ['select','correct'])test(`committed extraction reply cannot restore UI after external ${operation} callback`,async()=>{
 savedSummary='## Overview\nOld summary\n\n## Action items\n- Send API report';
 let release!:()=>void;actionReplyWait=new Promise<void>(resolve=>{release=resolve;});
 await mount();expect(actionStarted).toBe(true);expect(savedSummary).toBe('## Overview\nOld summary');
 // SQL already committed legitimately; only its JS reply is delayed. Exercise
 // the actual page invalidation callback without remounting the document.
 await act(async()=>{
  await transport(operation==='select'?'select_transcript_version':'correct_transcript_text',{text:'Corrected API'});
  const selector=root!.root.findAll(node=>typeof node.type==='function'&&node.type.name==='TranscriptVersions')[0];
  await selector.props.onChanged();
 });
 expect(root!.root.findAllByProps({'aria-label':'Summary editor'})).toHaveLength(0);
 await act(async()=>{release();});
 expect(root!.root.findAllByProps({'aria-label':'Summary editor'})).toHaveLength(0);expect(savedSummary).toBe('');
 const {captureMeetingExport}=await import('../../src/lib/meeting-export');expect((await captureMeetingExport(active)).summary).toBeNull();
},60000);


test('delayed extraction reply does not roll back newer task edits or anchors in the UI',async()=>{
 savedSummary='## Overview\nOld summary\n\n## Action items\n- Send API report';
 let release!:()=>void;actionReplyWait=new Promise<void>(resolve=>{release=resolve;});
 await mount();expect(actionStarted).toBe(true);
 actionRows=[{id:'curated',text:'User-curated task',done:true,edited:true,transcriptId:'current-anchor',audioTime:42}];
 await act(async()=>root!.root.findByProps({'aria-label':'Action rows'}).props.onClick());
 expect(visible(root!.toJSON())).toContain('User-curated task');
 await act(async()=>{release();});await ready();
 expect(visible(root!.toJSON())).toContain('User-curated task');expect(visible(root!.toJSON())).toContain('current-anchor');
},60000);


test('navigation cancels old extraction UI completion without blocking the new meeting version barrier',async()=>{
 savedSummary='## Overview\nOld summary\n\n## Action items\n- Send API report';
 let release!:()=>void;actionReplyWait=new Promise<void>(resolve=>{release=resolve;});
 let work:Promise<void>|undefined;
 try{
  await mount();expect(actionStarted).toBe(true);
  active=`combined-${++seq}`;savedSummary='Meeting B summary';summaryToken++;
  documents.set(`meeting:${active}`,{markdown:'Meeting B notes',json:null,revision:1,source:'user_notes'});
  await act(async()=>navigateView(active,savedSummary));await ready();
  await review('select');await act(async()=>{work=mutate('select');});
  expect(mutations).toBe(1);
  await act(async()=>{release();await work;});
  expect(savedSummary).toBe('');expect(archivedSummary).toBe('Meeting B summary');
  expect(root!.root.findAllByProps({'aria-label':'Summary editor'})).toHaveLength(0);
  expect(editor('Notes editor').props.value).toContain('Meeting B notes');
 }finally{await act(async()=>{release();await work;});}
},60000);


const publishCanonical=()=>{
 canonicalSummary={markdown:'## Summary\n\nCanonical report discussed',post_call:{source:'post_call_results',job_epoch:3,version_id:'live',transcript_revision:7,notes_revision:1,vocabulary_revision:0}};
 savedSummary=canonicalSummary.markdown;summaryExists=true;summaryToken++;
 actionRows=[{id:'generated-task',text:'Send report',done:false,source:'ai',transcriptId:'old-line',audioTime:1}];
};

test('canonical native publication refreshes normal detail editor/tasks/library and both exports',async()=>{
 savedSummary='';summaryExists=false;await mount();
 const notesIdentity=editor('Notes editor').props['data-engine'];
 await act(async()=>{publishCanonical();for(const event of nativeEvents)event();});await ready();
 expect(editor('Summary editor').props.value).toContain('Canonical report discussed');
 expect(visible(root!.toJSON())).toContain('Send report');expect(editor('Notes editor').props['data-engine']).toBe(notesIdentity);
 await detailExport();expect(written).toHaveLength(1);expect(written[0].text).toContain('Canonical report discussed');expect(written[0].text).toContain('Send report');
 await act(async()=>root!.update(<MeetingsPage/>));await search('Canonical');
 expect(visible(root!.toJSON())).toContain('Canonical report discussed');
 await act(async()=>exactButton('Export…').props.onClick());await act(async()=>exactButton('Export',true).props.onClick());
 expect(written).toHaveLength(2);expect(written[1].text).toContain('Canonical report discussed');expect(written[1].text).toContain('Send report');expect(written[0].text).toContain('job epoch 3');expect(written[1].text).toContain('Notes revision 1');
},60000);

test('canonical late refresh cannot replace an immediate rich summary draft or navigated meeting',async()=>{
 await mount();const summaryIdentity=editor('Summary editor').props['data-engine'];
 let release!:()=>void;canonicalWait=new Promise<void>(resolve=>{release=resolve;});
 await act(async()=>{publishCanonical();for(const event of nativeEvents)event();});
 holdSummary=true;await act(async()=>editor('Summary editor').props.onChange({target:{blocks:rich}}));
 await act(async()=>{release();root!.update(<Page/>);});expect(editor('Summary editor').props['data-engine']).toBe(summaryIdentity);expect(editor('Summary editor').props.value).toContain('API follow-up');
 holdSummary=false;await act(async()=>{for(const item of conversions.splice(0))item.resolve(richMarkdown);});
 canonicalWait=new Promise<void>(resolve=>{release=resolve;});
 await act(async()=>{for(const event of nativeEvents)event();});
 active=`combined-${++seq}`;documents.set(`meeting:${active}`,{markdown:'Meeting B notes',json:null,revision:1,source:'user_notes'});
 canonicalSummary=null;savedSummary='Meeting B summary';await act(async()=>{navigateView(active,savedSummary);release();});await ready();
 expect(editor('Summary editor').props.value).toContain('Meeting B summary');expect(editor('Summary editor').props.value).not.toContain('Canonical report');
},60000);
